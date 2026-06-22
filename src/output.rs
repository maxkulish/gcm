//! Machine-facing JSON contract for `--json` mode (CLO-493).
//!
//! Every `--json` invocation emits exactly one JSON object on stdout (schema
//! version `v: 1`). Human-oriented prose, warnings, and debug logs are kept
//! on stderr so JSON consumers can parse stdout without filtering.

use serde::Serialize;

use crate::error::GcmError;
use crate::plan::Plan;
use crate::provider::{ErrorKind, ProviderError};

pub const SCHEMA_VERSION: i32 = 1;

pub const STATUS_PLAN: &str = "plan";
pub const STATUS_NOOP: &str = "noop";
pub const STATUS_COMMITTED: &str = "committed";
pub const STATUS_FALLBACK: &str = "fallback";
pub const STATUS_ERROR: &str = "error";

pub const MODE_PLAN_ONLY: &str = "plan_only";
pub const MODE_DRY_RUN: &str = "dry_run";
pub const MODE_SINGLE: &str = "single";
pub const MODE_GROUPED: &str = "grouped";

/// A single machine-readable outcome. Fields are omitted when irrelevant so
/// consumers can rely on the shape implied by `status`.
#[derive(Debug, Serialize)]
pub struct Envelope {
    pub v: i32,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitSummary {
    pub status: &'static str,
    pub hash: String,
    pub message: String,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FallbackInfo {
    pub reason: String,
    pub raw_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitSummary>,
}

#[derive(Debug, Serialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

impl Envelope {
    fn base(
        status: &'static str,
        mode: Option<&'static str>,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Envelope {
            v: SCHEMA_VERSION,
            status,
            mode,
            provider: provider.map(|s| s.to_string()),
            model: model.map(|s| s.to_string()),
            plan: None,
            changed_files: None,
            cached: None,
            commit: None,
            fallback: None,
            error: None,
        }
    }
}

/// A clean repository: nothing to do.
pub fn noop(provider: Option<&str>, model: Option<&str>, mode: &'static str) -> Envelope {
    Envelope::base(STATUS_NOOP, Some(mode), provider, model)
}

/// A preview outcome (`--plan-only` or `--dry-run`).
pub fn plan(
    provider: Option<&str>,
    model: Option<&str>,
    mode: &'static str,
    plan: Plan,
    changed_files: Vec<String>,
    cached: bool,
) -> Envelope {
    let mut env = Envelope::base(STATUS_PLAN, Some(mode), provider, model);
    env.plan = Some(plan);
    env.changed_files = Some(changed_files);
    env.cached = Some(cached);
    env
}

/// One group (or the whole tree in `--all` mode) was committed.
pub fn committed(
    provider: Option<&str>,
    model: Option<&str>,
    mode: &'static str,
    hash: String,
    message: String,
    changed_files: Vec<String>,
) -> Envelope {
    let mut env = Envelope::base(STATUS_COMMITTED, Some(mode), provider, model);
    env.commit = Some(CommitSummary {
        status: "ok",
        hash,
        message,
        changed_files,
    });
    env
}

/// Grouping failed and the single-commit fallback succeeded.
pub fn fallback(
    provider: Option<&str>,
    model: Option<&str>,
    reason: String,
    raw_code: String,
    commit: CommitSummary,
) -> Envelope {
    let mut env = Envelope::base(STATUS_FALLBACK, Some(MODE_GROUPED), provider, model);
    env.fallback = Some(FallbackInfo {
        reason,
        raw_code,
        commit: Some(commit.clone()),
    });
    env.commit = Some(commit);
    env
}

/// A runtime error.
pub fn error(
    provider: Option<&str>,
    model: Option<&str>,
    mode: Option<&'static str>,
    err: &GcmError,
) -> Envelope {
    let mut env = Envelope::base(STATUS_ERROR, mode, provider, model);
    env.error = Some(ErrorInfo {
        code: gcm_error_code(err),
        message: err.to_string(),
    });
    env
}

/// Serialize and emit the envelope to stdout. This is the only place `--json`
/// writes to stdout.
pub fn emit(env: &Envelope) {
    println!(
        "{}",
        serde_json::to_string(env).unwrap_or_else(|_| {
            // serde_json cannot fail on our types in practice; fall back to a
            // minimal guaranteed-valid envelope rather than panic in automation.
            "{\"v\":1,\"status\":\"error\",\"error\":{\"code\":\"Internal\",\"message\":\"failed to serialize outcome\"}}"
                .to_string()
        })
    );
}

/// Stable top-level `error.code` for `GcmError` variants.
fn gcm_error_code(err: &GcmError) -> String {
    match err {
        GcmError::NotARepo => "NotARepo".to_string(),
        GcmError::Git(_) => "Git".to_string(),
        GcmError::Provider(_) => "Provider".to_string(),
        GcmError::NonInteractive => "NonInteractive".to_string(),
        GcmError::Editor(_) => "Editor".to_string(),
        GcmError::EmptyMessage => "EmptyMessage".to_string(),
        GcmError::UnmergedConflicts => "UnmergedConflicts".to_string(),
        GcmError::CommitFailed(_) => "CommitFailed".to_string(),
        GcmError::Config(_) => "Config".to_string(),
        GcmError::SecretDetected { .. } => "SecretDetected".to_string(),
    }
}

/// Machine-readable code for a provider failure, used inside `fallback.raw_code`.
pub fn provider_error_code(err: &ProviderError) -> String {
    match &err.kind {
        ErrorKind::MissingKey { .. } => "MissingKey".to_string(),
        ErrorKind::RateLimit { .. } => "RateLimit".to_string(),
        ErrorKind::Auth { status, .. } => format!("Auth{status}"),
        ErrorKind::BadRequest { .. } => "BadRequest".to_string(),
        ErrorKind::Server(code) => format!("Server{code}"),
        ErrorKind::Http(code) => format!("Http{code}"),
        ErrorKind::Timeout => "Timeout".to_string(),
        ErrorKind::Transport(_) => "Transport".to_string(),
        ErrorKind::EmptyResponse => "EmptyResponse".to_string(),
        ErrorKind::Deserialize(_) => "Deserialize".to_string(),
        ErrorKind::Config(_) => "Config".to_string(),
    }
}

/// Build a `fallback.raw_code` from either a provider error or a plain reason
/// string. Provider errors get a typed code; plan-validation/parse fallbacks use
/// `"PlanValidation"` / `"Deserialize"` derived from the reason text.
pub fn fallback_raw_code(reason: &str) -> String {
    if reason.contains("plan parse error") {
        "Deserialize".to_string()
    } else {
        "PlanValidation".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderError;

    #[test]
    fn noop_has_version_and_status() {
        let env = noop(Some("Groq"), Some("m"), MODE_PLAN_ONLY);
        assert_eq!(env.v, 1);
        assert_eq!(env.status, STATUS_NOOP);
        assert_eq!(env.mode, Some(MODE_PLAN_ONLY));
    }

    #[test]
    fn plan_includes_plan_and_changed_files() {
        let p = Plan {
            groups: vec![crate::plan::Group {
                files: vec!["a.rs".to_string()],
                summary: "s".to_string(),
                commit_message: Some("feat: a".to_string()),
            }],
        };
        let env = plan(
            Some("Groq"),
            Some("m"),
            MODE_DRY_RUN,
            p,
            vec!["a.rs".to_string()],
            false,
        );
        assert_eq!(env.status, STATUS_PLAN);
        assert_eq!(env.mode, Some(MODE_DRY_RUN));
        assert!(env.plan.is_some());
        assert_eq!(
            env.changed_files.as_deref(),
            Some(["a.rs".to_string()].as_slice())
        );
        assert_eq!(env.cached, Some(false));
    }

    #[test]
    fn committed_envelope_has_commit_fields() {
        let env = committed(
            Some("Groq"),
            Some("m"),
            MODE_GROUPED,
            "abc123".to_string(),
            "feat: x".to_string(),
            vec!["x.rs".to_string()],
        );
        let c = env.commit.unwrap();
        assert_eq!(c.status, "ok");
        assert_eq!(c.hash, "abc123");
        assert_eq!(c.message, "feat: x");
    }

    #[test]
    fn fallback_envelope_includes_both_fallback_and_commit() {
        let commit = CommitSummary {
            status: "ok",
            hash: "abc".to_string(),
            message: "feat: x".to_string(),
            changed_files: vec!["x.rs".to_string()],
        };
        let env = fallback(
            Some("Groq"),
            Some("m"),
            "plan rejected".to_string(),
            "PlanValidation".to_string(),
            commit.clone(),
        );
        assert_eq!(env.status, STATUS_FALLBACK);
        assert!(env.fallback.is_some());
        assert_eq!(env.commit.as_ref().unwrap().hash, "abc");
    }

    #[test]
    fn error_maps_gcm_codes() {
        let env = error(None, None, None, &GcmError::NonInteractive);
        assert_eq!(env.error.as_ref().unwrap().code, "NonInteractive");
        let env = error(None, None, None, &GcmError::SecretDetected { count: 1 });
        assert_eq!(env.error.as_ref().unwrap().code, "SecretDetected");
    }

    #[test]
    fn provider_error_code_variants_distinct() {
        use crate::provider::ErrorKind;
        let codes: Vec<String> = vec![
            provider_error_code(&ProviderError {
                provider: "Groq",
                kind: ErrorKind::MissingKey { env_var: "K" },
            }),
            provider_error_code(&ProviderError {
                provider: "Groq",
                kind: ErrorKind::RateLimit { retry_after: None },
            }),
            provider_error_code(&ProviderError {
                provider: "Groq",
                kind: ErrorKind::Auth {
                    status: 401,
                    env_var: "K",
                },
            }),
            provider_error_code(&ProviderError {
                provider: "Groq",
                kind: ErrorKind::BadRequest { detail: None },
            }),
            provider_error_code(&ProviderError {
                provider: "Groq",
                kind: ErrorKind::Server(500),
            }),
            provider_error_code(&ProviderError {
                provider: "Groq",
                kind: ErrorKind::Timeout,
            }),
            provider_error_code(&ProviderError {
                provider: "Groq",
                kind: ErrorKind::Transport("x".into()),
            }),
        ];
        assert_eq!(
            codes.len(),
            codes.iter().collect::<std::collections::HashSet<_>>().len()
        );
    }
}
