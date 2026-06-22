//! Ollama local backend (CLO-495, FR-56). Native `/api/chat` with a JSON-Schema
//! object in `format` (ADR-001 Decision 8): first-class structured output, no
//! OpenAI-envelope translation. Reads `message.content`, **ignores**
//! `message.thinking` (Ollama separates reasoning into its own field). Key-free:
//! no `Authorization` header (the privacy anchor - zero-egress with a local
//! model). Endpoint defaults to `http://localhost:11434`, overridable via
//! `GCM_OLLAMA_BASE_URL` (full URL) or the Ollama-native `OLLAMA_HOST`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::http::{self, HttpRequest};
use super::{ErrorKind, Provider, ProviderError};
use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;

const NAME: &str = "Ollama";
const DEFAULT_BASE_URL: &str = "http://localhost:11434";
const DEFAULT_PORT: &str = "11434";

pub struct Ollama {
    model: String,
}

impl Ollama {
    pub fn new(model: String) -> Self {
        Ollama { model }
    }

    /// Endpoint base: `GCM_OLLAMA_BASE_URL` (gcm convention, full URL) >
    /// `OLLAMA_HOST` (Ollama-native, normalized) > default. Empty/whitespace
    /// values are treated as unset.
    fn base_url(&self) -> String {
        resolve_base_url(
            env_nonempty("GCM_OLLAMA_BASE_URL"),
            env_nonempty("OLLAMA_HOST"),
        )
    }

    fn endpoint(&self) -> String {
        format!("{}/api/chat", self.base_url().trim_end_matches('/'))
    }

    fn request<'a>(&self, payload: &'a Value) -> HttpRequest<'a> {
        HttpRequest {
            provider: NAME,
            auth_env_var: "", // no auth -> unreachable in an Auth (401/403) error
            endpoint: self.endpoint(),
            auth: None, // key-free: no Authorization header (zero-egress anchor)
            payload,
        }
    }

    /// POST a chat payload and extract `message.content`, remapping setup
    /// failures (unreachable daemon, missing model) into actionable messages.
    fn chat(&self, payload: &Value) -> Result<String, ProviderError> {
        let raw = http::post_json(&self.request(payload)).map_err(|e| self.remap_error(e))?;
        extract_content(&raw)
    }

    /// Remap setup-oriented failures into actionable messages built from the
    /// model + endpoint (no daemon body needed); other kinds pass through.
    /// Variant choice is driven by which `Display` renders the message cleanly
    /// (see `super::ProviderError`): `Transport` keeps its natural "could not
    /// reach …" prefix; the 404 uses `Config` (verbatim) since `Http(404)`
    /// carries no string and `BadRequest` appends "…please report it".
    fn remap_error(&self, err: ProviderError) -> ProviderError {
        let endpoint = self.endpoint();
        match err.kind {
            ErrorKind::Transport(cause) => ProviderError {
                provider: NAME,
                kind: ErrorKind::Transport(format!(
                    "{endpoint} - is Ollama running? Start it with `ollama serve`, or set OLLAMA_HOST. ({cause})"
                )),
            },
            ErrorKind::Http(404) => ProviderError {
                provider: NAME,
                kind: ErrorKind::Config(format!(
                    "Ollama model '{model}' not found at {endpoint} (HTTP 404). \
                     Pull it with `ollama pull {model}`, or pick another with --model / GCM_OLLAMA_MODEL.",
                    model = self.model
                )),
            },
            other => ProviderError {
                provider: NAME,
                kind: other,
            },
        }
    }
}

impl Provider for Ollama {
    fn name(&self) -> &'static str {
        NAME
    }

    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
        let payload = build_plan_payload(ctx, &self.model);
        let json = self.chat(&payload)?;
        if json.is_empty() {
            return Err(ProviderError {
                provider: NAME,
                kind: ErrorKind::EmptyResponse,
            });
        }
        crate::plan::parse_defensive(&json).map_err(|e| ProviderError {
            provider: NAME,
            kind: ErrorKind::Deserialize(e.to_string()),
        })
    }

    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
        let payload = build_message_payload(&self.model, &super::message_user_content(diff));
        let message = self.chat(&payload)?;
        if message.is_empty() {
            return Err(ProviderError {
                provider: NAME,
                kind: ErrorKind::EmptyResponse,
            });
        }
        Ok(message)
    }

    fn cache_model_id(&self) -> String {
        format!("ollama:{}", self.model)
    }

    fn diff_budget(&self) -> DiffBudget {
        // Local context windows vary by pulled model; standard budget for v1,
        // env-overridable (FR-13a).
        DiffBudget::standard()
    }
}

/// Read a non-empty, trimmed env var, else `None`.
fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Resolve the base URL from the two env sources (precedence
/// `GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` > default). Pure: env reads happen in
/// `base_url`, so this is unit-testable without touching process env.
fn resolve_base_url(gcm_base: Option<String>, ollama_host: Option<String>) -> String {
    if let Some(u) = gcm_base {
        return u;
    }
    if let Some(h) = ollama_host {
        return normalize_host(&h);
    }
    DEFAULT_BASE_URL.to_string()
}

/// Normalize an `OLLAMA_HOST` value into a base URL. A value with no `://` scheme
/// gets `http://` prepended; if it then carries no explicit port, the Ollama
/// default `:11434` is appended. A value that already has a scheme is taken
/// as-is (no port forced).
fn normalize_host(host: &str) -> String {
    let h = host.trim();
    if h.contains("://") {
        return h.to_string();
    }
    if has_port(h) {
        format!("http://{h}")
    } else {
        format!("http://{h}:{DEFAULT_PORT}")
    }
}

/// Whether a scheme-less host string carries an explicit numeric port in its
/// last `:`-segment (`host` -> false, `host:11434` -> true).
fn has_port(h: &str) -> bool {
    match h.rsplit_once(':') {
        Some((_, port)) => !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

fn build_plan_payload(ctx: &GroupingContext, model: &str) -> Value {
    json!({
        "model": model,
        "stream": false,
        "format": crate::plan::schema(),
        "options": { "temperature": 0.2 },
        "messages": [
            { "role": "system", "content": super::GROUPING_SYSTEM_PROMPT },
            { "role": "user", "content": super::grouping_user_content(ctx) },
        ],
    })
}

fn build_message_payload(model: &str, user_content: &str) -> Value {
    json!({
        "model": model,
        "stream": false,
        "options": { "temperature": 0.2 },
        "messages": [
            { "role": "system", "content": super::SYSTEM_PROMPT },
            { "role": "user", "content": user_content },
        ],
    })
}

/// Ollama's non-streaming `/api/chat` response: a single object with a top-level
/// `message`. `thinking` is intentionally absent from the struct (ignored).
#[derive(Deserialize)]
struct ChatResponse {
    message: Option<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

/// Parse the response: read `message.content`, apply the `<think>` backstop,
/// trim. A body missing `message` entirely is a `Deserialize` error (not
/// silently empty); empty/absent content yields `Ok("")` (the caller maps that
/// to `EmptyResponse`).
fn extract_content(raw: &str) -> Result<String, ProviderError> {
    let parsed: ChatResponse = serde_json::from_str(raw).map_err(|e| ProviderError {
        provider: NAME,
        kind: ErrorKind::Deserialize(e.to_string()),
    })?;
    let Some(message) = parsed.message else {
        return Err(ProviderError {
            provider: NAME,
            kind: ErrorKind::Deserialize("Ollama response missing 'message' key".to_string()),
        });
    };
    let content = message.content.unwrap_or_default();
    Ok(super::strip_think(&content).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> GroupingContext {
        GroupingContext {
            file_list: "a.rs".to_string(),
            status: " M a.rs".to_string(),
            stat: "1 file".to_string(),
            body: "diff --git a/a.rs b/a.rs".to_string(),
        }
    }

    #[test]
    fn plan_payload_is_native_format_schema_non_streaming() {
        let p = build_plan_payload(&ctx(), "gemma4:e4b-mlx");
        assert_eq!(p["model"], json!("gemma4:e4b-mlx"));
        assert_eq!(p["stream"], json!(false));
        assert_eq!(p["options"]["temperature"], json!(0.2));
        // native `format` carries the plain JSON-Schema object (not an OpenAI envelope)
        assert!(p["format"]["properties"]["groups"].is_object());
        assert_eq!(p["messages"][0]["role"], json!("system"));
        let user = p["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("Changed files"));
        assert!(user.contains("a.rs"));
    }

    #[test]
    fn message_payload_has_no_format_and_is_non_streaming() {
        let p = build_message_payload("gemma4:e4b-mlx", "Diff stats:\nx\n\nFull diff:\ny");
        assert_eq!(p["stream"], json!(false));
        assert!(p.get("format").is_none());
        assert_eq!(p["options"]["temperature"], json!(0.2));
        assert_eq!(p["messages"][0]["role"], json!("system"));
    }

    #[test]
    fn extract_reads_content_ignores_thinking_strips_think() {
        // thinking present + a <think> span in content: both excluded.
        let raw = r#"{"message":{"role":"assistant","thinking":"reasoning","content":"<think>x</think>  {\"groups\":[]}  "}}"#;
        assert_eq!(extract_content(raw).unwrap(), r#"{"groups":[]}"#);
    }

    #[test]
    fn extract_empty_or_absent_content_is_empty_string() {
        assert_eq!(
            extract_content(r#"{"message":{"content":""}}"#).unwrap(),
            ""
        );
        assert_eq!(
            extract_content(r#"{"message":{"role":"assistant"}}"#).unwrap(),
            ""
        );
    }

    #[test]
    fn extract_missing_message_key_is_deserialize_error() {
        let e = extract_content(r#"{"model":"gemma4:e4b-mlx","done":true}"#).unwrap_err();
        match e.kind {
            ErrorKind::Deserialize(m) => assert!(m.contains("message"), "got: {m}"),
            other => panic!("expected Deserialize, got {other:?}"),
        }
    }

    #[test]
    fn resolve_base_url_precedence_and_normalization() {
        // GCM_OLLAMA_BASE_URL wins over OLLAMA_HOST
        assert_eq!(
            resolve_base_url(
                Some("http://gcm.base:1".to_string()),
                Some("other:2".to_string())
            ),
            "http://gcm.base:1"
        );
        // port-less OLLAMA_HOST -> default 11434 appended
        assert_eq!(
            resolve_base_url(None, Some("localhost".to_string())),
            "http://localhost:11434"
        );
        // explicit port preserved
        assert_eq!(
            resolve_base_url(None, Some("127.0.0.1:8080".to_string())),
            "http://127.0.0.1:8080"
        );
        // explicit scheme taken as-is (no port forced)
        assert_eq!(
            resolve_base_url(None, Some("https://h.example".to_string())),
            "https://h.example"
        );
        // neither -> default
        assert_eq!(resolve_base_url(None, None), DEFAULT_BASE_URL);
    }

    #[test]
    fn normalize_host_variants() {
        assert_eq!(normalize_host("localhost"), "http://localhost:11434");
        assert_eq!(normalize_host("127.0.0.1:11434"), "http://127.0.0.1:11434");
        assert_eq!(
            normalize_host("http://127.0.0.1:11434"),
            "http://127.0.0.1:11434"
        );
        assert_eq!(
            normalize_host("my-host.local"),
            "http://my-host.local:11434"
        );
    }

    #[test]
    fn cache_model_id_is_provider_qualified() {
        let o = Ollama::new("gemma4:e4b-mlx".to_string());
        assert_eq!(o.cache_model_id(), "ollama:gemma4:e4b-mlx");
    }

    #[test]
    fn remap_unreachable_is_actionable_transport() {
        let o = Ollama::new("gemma4:e4b-mlx".to_string());
        let err = ProviderError {
            provider: NAME,
            kind: ErrorKind::Transport("connection refused".to_string()),
        };
        let msg = o.remap_error(err).to_string();
        assert!(msg.contains("is Ollama running"), "got: {msg}");
        assert!(msg.contains("OLLAMA_HOST"), "got: {msg}");
        // still the truthful Transport prefix
        assert!(msg.contains("could not reach the Ollama API"), "got: {msg}");
    }

    #[test]
    fn remap_404_is_actionable_config_pull() {
        let o = Ollama::new("gemma4:e4b-mlx".to_string());
        let err = ProviderError {
            provider: NAME,
            kind: ErrorKind::Http(404),
        };
        let remapped = o.remap_error(err);
        assert!(matches!(remapped.kind, ErrorKind::Config(_)));
        let msg = remapped.to_string();
        assert!(msg.contains("gemma4:e4b-mlx"), "got: {msg}");
        assert!(msg.contains("ollama pull"), "got: {msg}");
        // Config renders verbatim: none of the other variants' prefixes/suffixes
        assert!(!msg.contains("please report it"), "got: {msg}");
        assert!(!msg.contains("could not reach"), "got: {msg}");
    }

    #[test]
    fn remap_passes_through_other_kinds() {
        let o = Ollama::new("m".to_string());
        let err = ProviderError {
            provider: NAME,
            kind: ErrorKind::Server(500),
        };
        assert!(matches!(o.remap_error(err).kind, ErrorKind::Server(500)));
    }
}
