//! Google Vertex AI backend (CLO-537). A thin backend over the *identical* Gemini
//! `generateContent` payloads (reused from [`super::gemini`]): only the endpoint URL
//! and auth differ. Auth is **keyless** - a short-lived OAuth token from Application
//! Default Credentials, acquired by shelling out to `gcloud` (matching gcm's optional
//! external-binary pattern; `GCM_VERTEX_TOKEN` escape hatch first). Because it reuses
//! `gemini::build_resolve_payload`, it inherits the CLO-534 OpenAPI-subset resolve
//! schema (no `additionalProperties`) for free.
//!
//! The token is resolved **lazily** at call time (like `gemini::api_key`), so cache
//! resolution and `--dry-run` never spawn gcloud.

use std::io;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::Value;

use super::gemini;
use super::http::{self, HttpRequest};
use super::{ErrorKind, Provider, ProviderError};
use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;

const NAME: &str = "Vertex";
const TOKEN_ENV: &str = "GCM_VERTEX_TOKEN";
const PROJECT_ENV: &str = "GCM_VERTEX_PROJECT";
const LOCATION_ENV: &str = "GCM_VERTEX_LOCATION";
const BASE_URL_ENV: &str = "GCM_VERTEX_BASE_URL";
const DEFAULT_LOCATION: &str = "global";

/// Bound the gcloud token subprocess. git is invoked without a timeout because it is
/// local/instant, but a gcloud ADC refresh can block on the network (design §4.2 A3).
const GCLOUD_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Vertex {
    model: String,
}

impl Vertex {
    pub fn new(model: String) -> Self {
        Vertex { model }
    }

    /// Acquire the ADC access token: `GCM_VERTEX_TOKEN` (trimmed, non-empty) wins,
    /// else shell out to gcloud. Resolved lazily per call.
    fn access_token(&self) -> Result<String, ProviderError> {
        if let Some(tok) = env_nonblank(TOKEN_ENV) {
            return Ok(tok);
        }
        gcloud_token()
    }

    /// GCP project: `GCM_VERTEX_PROJECT` > `GOOGLE_CLOUD_PROJECT` > `GCP_PROJECT`.
    /// Required (no default); validated before it is templated into the URL path.
    fn project(&self) -> Result<String, ProviderError> {
        let p =
            first_env(&[PROJECT_ENV, "GOOGLE_CLOUD_PROJECT", "GCP_PROJECT"]).ok_or_else(|| {
                config_err(
                    "Vertex project not set. Set GCM_VERTEX_PROJECT (or GOOGLE_CLOUD_PROJECT), \
                 or run `gcm provider` to configure it."
                        .to_string(),
                )
            })?;
        validate_project(&p)?;
        Ok(p)
    }

    /// Vertex location: `GCM_VERTEX_LOCATION` > `GOOGLE_CLOUD_LOCATION` > `GCP_REGION`,
    /// default `global` (Gemini 3.x is global-only on Vertex). Validated strictly
    /// because it is templated into the endpoint host.
    fn location(&self) -> Result<String, ProviderError> {
        let loc = first_env(&[LOCATION_ENV, "GOOGLE_CLOUD_LOCATION", "GCP_REGION"])
            .unwrap_or_else(|| DEFAULT_LOCATION.to_string());
        validate_location(&loc)?;
        Ok(loc)
    }

    /// Endpoint base: `GCM_VERTEX_BASE_URL` (test seam) wins; else the global host for
    /// `global`, or the regional `{location}-aiplatform` host otherwise.
    fn base_url(&self, location: &str) -> String {
        if let Some(u) = env_nonblank(BASE_URL_ENV) {
            return u.trim_end_matches('/').to_string();
        }
        if location == DEFAULT_LOCATION {
            "https://aiplatform.googleapis.com".to_string()
        } else {
            format!("https://{location}-aiplatform.googleapis.com")
        }
    }

    fn request<'a>(
        &self,
        token: &str,
        project: &str,
        location: &str,
        payload: &'a Value,
    ) -> HttpRequest<'a> {
        let base = self.base_url(location);
        HttpRequest {
            provider: NAME,
            // The token rides in `extra_headers` and `auth` is None, so a 401/403 is
            // classified as `Http(status)` (not `Auth{env_var}`); `map_auth_error`
            // then rewrites it with a gcloud-specific hint (design §4.2 N1). `""` is
            // the no-auth placeholder convention.
            auth_env_var: "",
            endpoint: format!(
                "{base}/v1/projects/{project}/locations/{location}/publishers/google/models/{}:generateContent",
                self.model
            ),
            auth: None,
            extra_headers: vec![("Authorization", format!("Bearer {token}"))],
            payload,
        }
    }

    /// Resolve token + project + location once for a request (the common prologue of
    /// all three trait methods).
    fn target(&self) -> Result<(String, String, String), ProviderError> {
        Ok((self.access_token()?, self.project()?, self.location()?))
    }

    /// Re-map a raw HTTP auth failure to Vertex-specific, actionable text. A Bearer
    /// 401/403 is meaningless as "check <env_var>"; 403 on Vertex usually means IAM
    /// denied or the API is not enabled, not a bad credential.
    fn map_auth_error(&self, e: ProviderError) -> ProviderError {
        match e.kind {
            ErrorKind::Http(401) => config_err(
                "Vertex rejected the credential (HTTP 401): the ADC token is invalid or \
                 expired. Run: gcloud auth application-default login"
                    .to_string(),
            ),
            ErrorKind::Http(403) => {
                let project = self
                    .project()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|_| "<project>".to_string());
                config_err(format!(
                    "Vertex denied access (HTTP 403) on project '{project}'. Check IAM \
                     (grant roles/aiplatform.user), and that the Vertex AI API is enabled: \
                     gcloud services enable aiplatform.googleapis.com --project {project}"
                ))
            }
            _ => e,
        }
    }
}

impl Provider for Vertex {
    fn name(&self) -> &'static str {
        NAME
    }

    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
        let (token, project, location) = self.target()?;
        let payload = gemini::build_plan_payload(ctx);
        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
            .map_err(|e| self.map_auth_error(e))?;
        let json = gemini::extract_text(&raw)?;
        if json.is_empty() {
            return Err(empty());
        }
        crate::plan::parse_defensive(&json).map_err(|e| ProviderError {
            provider: NAME,
            kind: ErrorKind::Deserialize(e.to_string()),
        })
    }

    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
        let (token, project, location) = self.target()?;
        let payload = gemini::build_message_payload(&super::message_user_content(diff));
        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
            .map_err(|e| self.map_auth_error(e))?;
        let message = gemini::extract_text(&raw)?;
        if message.is_empty() {
            return Err(empty());
        }
        Ok(message)
    }

    fn resolve_hunks(
        &self,
        ctx: &super::ResolveContext,
    ) -> Result<Vec<super::Resolution>, ProviderError> {
        let (token, project, location) = self.target()?;
        let payload = gemini::build_resolve_payload(ctx);
        let raw = http::post_json(&self.request(&token, &project, &location, &payload))
            .map_err(|e| self.map_auth_error(e))?;
        let json = gemini::extract_text(&raw)?;
        if json.is_empty() {
            return Err(empty());
        }
        super::parse_resolutions(NAME, &json, ctx.hunks.len())
    }

    fn cache_model_id(&self) -> String {
        // Distinct from "google:{model}" so a cached plan from AI Studio never
        // satisfies a Vertex run (different endpoint + enterprise terms).
        format!("vertex:{}", self.model)
    }

    fn diff_budget(&self) -> DiffBudget {
        DiffBudget::standard()
    }
}

fn empty() -> ProviderError {
    ProviderError {
        provider: NAME,
        kind: ErrorKind::EmptyResponse,
    }
}

fn config_err(msg: String) -> ProviderError {
    ProviderError {
        provider: NAME,
        kind: ErrorKind::Config(msg),
    }
}

/// A trimmed, non-blank env var value, or `None`.
fn env_nonblank(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The first non-blank value among `vars`, in order.
fn first_env(vars: &[&str]) -> Option<String> {
    vars.iter().find_map(|v| env_nonblank(v))
}

/// Validate a Vertex location (templated into the endpoint HOST, so strict): either
/// `global` or a region like `us-central1` - a lowercase letter start, then
/// lowercase-alphanumeric or `-`. Rejects anything that could malform the host.
fn validate_location(loc: &str) -> Result<(), ProviderError> {
    let ok = loc == DEFAULT_LOCATION
        || (loc.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && loc
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
    if ok {
        Ok(())
    } else {
        Err(config_err(format!(
            "invalid Vertex location '{loc}': expected 'global' or a region like 'us-central1'"
        )))
    }
}

/// Validate a GCP project id (templated into the URL PATH segment, so lenient - only
/// reject characters that would break URL structure). This deliberately accepts legacy
/// domain-scoped ids like `example.com:my-project` (which contain `.` and `:`).
fn validate_project(project: &str) -> Result<(), ProviderError> {
    // Reject `%` too: a percent-encoded delimiter (e.g. `%2F`) would survive
    // templating and be decoded to `/` server-side, smuggling a path segment.
    let bad = project.is_empty()
        || project
            .chars()
            .any(|c| matches!(c, '/' | '?' | '#' | '%') || c.is_whitespace() || c.is_control());
    if bad {
        Err(config_err(format!(
            "invalid Vertex project '{project}': contains characters not allowed in a GCP project id"
        )))
    } else {
        Ok(())
    }
}

/// Shell out to `gcloud auth application-default print-access-token` under a bounded
/// timeout. Distinguishes "gcloud not installed" (io::ErrorKind::NotFound) from
/// "installed but ADC not initialized" so the two hints are correct (design §4.2 P8).
fn gcloud_token() -> Result<String, ProviderError> {
    let mut cmd = Command::new("gcloud");
    cmd.args(["auth", "application-default", "print-access-token"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(config_err(
                "gcloud not found: install the Google Cloud SDK (https://cloud.google.com/sdk), \
                 or set GCM_VERTEX_TOKEN with a valid access token."
                    .to_string(),
            ));
        }
        Err(e) => return Err(config_err(format!("failed to run gcloud: {e}"))),
    };

    // Bounded wait: a background thread drains stdout/stderr (avoids the pipe-fill
    // deadlock) and sends the output; the main thread gives up after GCLOUD_TIMEOUT.
    let (tx, rx) = mpsc::channel();
    let pid = child.id();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(GCLOUD_TIMEOUT) {
        Ok(Ok(out)) if out.status.success() => {
            let tok = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if tok.is_empty() {
                Err(config_err(
                    "gcloud returned an empty access token. Run: gcloud auth application-default login"
                        .to_string(),
                ))
            } else {
                Ok(tok)
            }
        }
        Ok(Ok(out)) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(config_err(reauth_hint(&stderr)))
        }
        Ok(Err(e)) => Err(config_err(format!("failed to wait on gcloud: {e}"))),
        Err(_) => {
            // Best-effort reap of the timed-out child.
            let _ = Command::new("kill").arg(pid.to_string()).status();
            Err(config_err(format!(
                "gcloud token request timed out after {GCLOUD_TIMEOUT:?}. Check your network, \
                 or run: gcloud auth application-default login"
            )))
        }
    }
}

/// Turn gcloud stderr into an actionable message, surfacing a reauth hint when the
/// failure looks like an expired/invalid grant.
fn reauth_hint(stderr: &str) -> String {
    let s = stderr.trim();
    let lower = s.to_lowercase();
    if lower.contains("invalid_grant")
        || lower.contains("reauth")
        || lower.contains("could not automatically determine credentials")
        || lower.contains("application default credentials")
    {
        return format!(
            "gcloud could not provide an access token (run: gcloud auth application-default login). Details: {s}"
        );
    }
    format!("gcloud failed to print an access token: {s}")
}

/// Wizard readiness probe (CLO-537): try to acquire an ADC token now, bounded by the
/// same timeout as the hot path. Returns `Ok(())` or a short human-readable reason.
/// Not used on the commit path (the wizard calls it once, non-blocking).
pub(super) fn probe_adc() -> Result<(), String> {
    Vertex::new(String::new())
        .access_token()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_url_global_uses_bare_aiplatform_host() {
        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
        let payload = serde_json::json!({});
        let req = v.request("tok", "my-proj", "global", &payload);
        assert_eq!(
            req.endpoint,
            "https://aiplatform.googleapis.com/v1/projects/my-proj/locations/global/publishers/google/models/gemini-3.1-flash-lite:generateContent"
        );
    }

    #[test]
    fn request_url_regional_uses_prefixed_host() {
        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
        let payload = serde_json::json!({});
        let req = v.request("tok", "my-proj", "us-central1", &payload);
        assert!(req
            .endpoint
            .starts_with("https://us-central1-aiplatform.googleapis.com/v1/projects/my-proj/locations/us-central1/"));
    }

    #[test]
    fn request_sends_bearer_via_extra_headers_and_no_auth() {
        // auth: None keeps classify_status from emitting Auth{env_var}; the token is
        // still sent (extra_headers), and map_auth_error handles 401/403.
        let v = Vertex::new("m".to_string());
        let payload = serde_json::json!({});
        let req = v.request("secret-token", "p", "global", &payload);
        assert!(req.auth.is_none());
        assert_eq!(
            req.extra_headers,
            vec![("Authorization", "Bearer secret-token".to_string())]
        );
    }

    #[test]
    fn cache_model_id_is_vertex_qualified() {
        let v = Vertex::new("gemini-3.1-flash-lite".to_string());
        assert_eq!(v.cache_model_id(), "vertex:gemini-3.1-flash-lite");
    }

    #[test]
    fn base_url_test_seam_overrides_host() {
        let v = Vertex::new("m".to_string());
        // With no env override, global -> bare host.
        assert_eq!(v.base_url("global"), "https://aiplatform.googleapis.com");
    }

    #[test]
    fn validate_location_accepts_global_and_regions() {
        assert!(validate_location("global").is_ok());
        assert!(validate_location("us-central1").is_ok());
        assert!(validate_location("europe-west4").is_ok());
    }

    #[test]
    fn validate_location_rejects_malformed() {
        assert!(validate_location("US-CENTRAL1").is_err()); // uppercase
        assert!(validate_location("us central1").is_err()); // space
        assert!(validate_location("../evil").is_err());
        assert!(validate_location("-leading").is_err()); // must start with a letter
    }

    #[test]
    fn validate_project_accepts_domain_scoped() {
        // Legacy domain-scoped ids must be accepted (round-2 review P2).
        assert!(validate_project("my-project-123").is_ok());
        assert!(validate_project("example.com:my-project").is_ok());
    }

    #[test]
    fn validate_project_rejects_url_structural_chars() {
        assert!(validate_project("").is_err());
        assert!(validate_project("a/b").is_err());
        assert!(validate_project("a b").is_err());
        assert!(validate_project("a?b").is_err());
        // Percent-encoded delimiter must be rejected before templating.
        assert!(validate_project("a%2Fb").is_err());
        assert!(validate_project("a%00").is_err());
    }

    #[test]
    fn map_auth_error_rewrites_401_403_to_actionable_text() {
        let v = Vertex::new("m".to_string());
        let e401 = v.map_auth_error(ProviderError {
            provider: NAME,
            kind: ErrorKind::Http(401),
        });
        assert!(e401.to_string().contains("application-default login"));
        let e403 = v.map_auth_error(ProviderError {
            provider: NAME,
            kind: ErrorKind::Http(403),
        });
        assert!(e403.to_string().contains("aiplatform.googleapis.com"));
        // Non-auth errors pass through unchanged.
        let other = v.map_auth_error(ProviderError {
            provider: NAME,
            kind: ErrorKind::Http(500),
        });
        assert!(matches!(other.kind, ErrorKind::Http(500)));
    }
}
