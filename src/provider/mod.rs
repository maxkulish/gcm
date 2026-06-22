//! Provider abstraction (CLO-489, FR-11): one synchronous trait (ADR-001
//! Decision 2 - blocking client, no async) that every LLM backend implements,
//! plus a flag/env registry (FR-12, precedence flag > env > default) and a
//! provider-agnostic error taxonomy generalized from CLO-488's `GroqError`.
//!
//! Backends: [`groq`] and [`openai`] share the OpenAI-compatible chat shape;
//! [`gemini`] uses Google's divergent `generateContent`/`responseSchema` shape.
//! Shared HTTP transport + retry/backoff (CLO-488) lives in [`http`].

mod anthropic;
mod gemini;
mod groq;
mod http;
mod openai;

use std::fmt;
use std::time::Duration;

use clap::ValueEnum;
use serde::Deserialize;

use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;

/// One LLM provider (FR-11). Synchronous (ADR-001 Decision 2). Both calls are
/// required: the structured grouping plan and the single commit message (tracer,
/// grouping fallback, and per-group message regeneration on an advanced cache hit).
pub trait Provider {
    /// Stable display name for messages/debug (e.g. "Groq" / "Google" / "OpenAI").
    fn name(&self) -> &'static str;
    /// Structured grouping plan; defensively parsed into a typed [`Plan`].
    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError>;
    /// A single conventional-commit message for the gathered diff.
    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError>;
    /// Provider-qualified model id folded into the cache freshness fingerprint
    /// (FR-27); resolvable with **no** API key (e.g. "groq:openai/gpt-oss-120b").
    fn cache_model_id(&self) -> String;
    /// Per-provider diff budget (FR-13a), env-overridable.
    fn diff_budget(&self) -> DiffBudget;
}

/// Typed, provider-agnostic failure taxonomy (FR-21). Carries the active provider
/// name so [`fmt::Display`] is specific without a separate variant per provider;
/// [`is_retryable`] decides which `kind`s are retried with bounded backoff (FR-22).
#[derive(Debug)]
pub struct ProviderError {
    pub provider: &'static str,
    pub kind: ErrorKind,
}

/// The failure category (generalized from CLO-488's `GroqError`). `MissingKey`
/// and `Auth` carry the exact env var so the message names the right key (FR-18).
#[derive(Debug)]
pub enum ErrorKind {
    /// The provider's API key env var is unset/blank (fatal, never retried).
    MissingKey { env_var: &'static str },
    /// HTTP 429 rate limit (retryable); `retry_after` from a `Retry-After` header.
    RateLimit { retry_after: Option<Duration> },
    /// HTTP 401/403: the API key was rejected (fatal).
    Auth { status: u16, env_var: &'static str },
    /// HTTP 400 or a content block (e.g. Gemini safety): not retried.
    BadRequest { detail: Option<String> },
    /// HTTP 5xx incl. 504 Gateway Timeout (retryable).
    Server(u16),
    /// Any other unexpected non-2xx status (not retried).
    Http(u16),
    /// Client-side request timeout (not retried).
    Timeout,
    /// Connection/transport failure - DNS, refused, reset (not retried).
    Transport(String),
    /// A 2xx response carried no usable content (not retried).
    EmptyResponse,
    /// The response/plan could not be parsed (not retried).
    Deserialize(String),
    /// A configuration error (e.g. an unknown provider name); fatal, not retried.
    Config(String),
}

impl ProviderError {
    fn new(provider: &'static str, kind: ErrorKind) -> Self {
        ProviderError { provider, kind }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = self.provider;
        match &self.kind {
            ErrorKind::MissingKey { env_var } => write!(
                f,
                "{p} API key is not set. Export it (e.g. `export {env_var}=...`) and retry."
            ),
            ErrorKind::RateLimit { .. } => write!(
                f,
                "{p} rate limit reached (HTTP 429); wait a moment and retry, or use a different provider."
            ),
            ErrorKind::Auth { status, env_var } => write!(
                f,
                "{p} rejected the API key (HTTP {status}); check that {env_var} is valid and not expired."
            ),
            ErrorKind::BadRequest { detail: Some(d) } => write!(
                f,
                "{p} rejected the request (HTTP 400): {d}. Likely an unsupported model/parameter or a gcm bug; please report it."
            ),
            ErrorKind::BadRequest { detail: None } => write!(
                f,
                "{p} rejected the request (HTTP 400). Likely an unsupported model/parameter or a gcm bug; please report it."
            ),
            ErrorKind::Server(code) => write!(
                f,
                "{p} server error (HTTP {code}); this is usually transient - retry shortly."
            ),
            ErrorKind::Http(code) => write!(f, "{p} API returned HTTP {code}"),
            ErrorKind::Timeout => write!(f, "{p} API request timed out"),
            ErrorKind::Transport(msg) => write!(f, "could not reach the {p} API: {msg}"),
            ErrorKind::EmptyResponse => write!(f, "{p} returned an empty response"),
            ErrorKind::Deserialize(msg) => write!(f, "could not parse the {p} response: {msg}"),
            ErrorKind::Config(msg) => write!(f, "{msg}"),
        }
    }
}

/// Which `kind`s are worth retrying (FR-22): only transient 429 / 5xx.
pub(crate) fn is_retryable(kind: &ErrorKind) -> bool {
    matches!(kind, ErrorKind::RateLimit { .. } | ErrorKind::Server(_))
}

/// The server's `Retry-After` hint, when the error carries one (429 only).
pub(crate) fn retry_after_hint(kind: &ErrorKind) -> Option<Duration> {
    match kind {
        ErrorKind::RateLimit { retry_after } => *retry_after,
        _ => None,
    }
}

/// Read a non-empty, parseable `u64` env var, else `None` (shared by submodules).
fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

// ---------------------------------------------------------------------------
// Provider selection (FR-12) and model resolution (FR-14)
// ---------------------------------------------------------------------------

/// The selectable providers. `--provider` accepts the lower-case names; `google`
/// also accepts the alias `gemini` (its API key is `GEMINI_API_KEY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ProviderId {
    Groq,
    #[value(alias = "gemini")]
    Google,
    Openai,
    Anthropic,
}

impl ProviderId {
    /// Default model id (ADR-001 Decisions 5/7 + capability matrix).
    fn default_model(self) -> &'static str {
        match self {
            ProviderId::Groq => "openai/gpt-oss-120b",
            ProviderId::Google => "gemini-3.1-flash-lite",
            ProviderId::Openai => "gpt-4o-mini-2024-07-18",
            ProviderId::Anthropic => "claude-haiku-4-5",
        }
    }

    /// Per-provider model env vars, in precedence order (primary first). Google
    /// reads both `GCM_GEMINI_MODEL` (primary, matches `GEMINI_API_KEY`) and the
    /// `GCM_GOOGLE_MODEL` alias (round-2 review pt 4).
    fn model_env_vars(self) -> &'static [&'static str] {
        match self {
            ProviderId::Groq => &["GCM_GROQ_MODEL"],
            ProviderId::Google => &["GCM_GEMINI_MODEL", "GCM_GOOGLE_MODEL"],
            ProviderId::Openai => &["GCM_OPENAI_MODEL"],
            ProviderId::Anthropic => &["GCM_ANTHROPIC_MODEL"],
        }
    }

    /// Parse a provider name (env), case- and whitespace-insensitive, honoring
    /// the `gemini` alias.
    fn parse(s: &str) -> Option<Self> {
        <ProviderId as ValueEnum>::from_str(s.trim(), true).ok()
    }
}

/// Resolve and construct the selected provider (FR-12/FR-14). Pure w.r.t. the API
/// key (keys are read lazily inside `generate_*`), so the cache path and
/// `--dry-run` resolve a provider without a key.
pub fn select(
    cli_provider: Option<ProviderId>,
    cli_model: Option<&str>,
) -> Result<Box<dyn Provider>, ProviderError> {
    let id = resolve_provider_id(cli_provider)?;
    let model = resolve_model(id, cli_model);
    Ok(match id {
        ProviderId::Groq => Box::new(groq::Groq::new(model)),
        ProviderId::Google => Box::new(gemini::Gemini::new(model)),
        ProviderId::Openai => Box::new(openai::OpenAi::new(model)),
        ProviderId::Anthropic => Box::new(anthropic::Anthropic::new(model)),
    })
}

fn resolve_provider_id(cli: Option<ProviderId>) -> Result<ProviderId, ProviderError> {
    let env = std::env::var("GCM_PROVIDER").ok();
    pick_provider_id(cli, env.as_deref())
}

/// Precedence flag > env > default(groq). An empty/whitespace `GCM_PROVIDER` is
/// treated as unset (round-2 review pt 4); a non-empty unknown name is a fatal
/// config error listing the valid names.
fn pick_provider_id(
    cli: Option<ProviderId>,
    env_raw: Option<&str>,
) -> Result<ProviderId, ProviderError> {
    if let Some(id) = cli {
        return Ok(id);
    }
    match env_raw {
        None => Ok(ProviderId::Groq),
        Some(raw) => {
            let t = raw.trim();
            if t.is_empty() {
                return Ok(ProviderId::Groq);
            }
            ProviderId::parse(t).ok_or_else(|| {
                ProviderError::new(
                    "gcm",
                    ErrorKind::Config(format!(
                        "unknown provider '{t}'. Set --provider/GCM_PROVIDER to one of: groq, google, openai, anthropic."
                    )),
                )
            })
        }
    }
}

fn resolve_model(id: ProviderId, cli: Option<&str>) -> String {
    let env_vals: Vec<Option<String>> = id
        .model_env_vars()
        .iter()
        .map(|v| std::env::var(v).ok())
        .collect();
    pick_model(cli, &env_vals, id.default_model())
}

/// Precedence flag > per-provider env (in order) > default. Empty/whitespace
/// values (flag or env) are skipped, never treated as a literal model id
/// (round-2 review / Gemini P1.5).
fn pick_model(cli: Option<&str>, env_vals: &[Option<String>], default: &str) -> String {
    if let Some(m) = cli.map(str::trim).filter(|m| !m.is_empty()) {
        return m.to_string();
    }
    for v in env_vals {
        if let Some(m) = v.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            return m.to_string();
        }
    }
    default.to_string()
}

// ---------------------------------------------------------------------------
// Shared OpenAI-compatible chat helpers (Groq + OpenAI) and the universal
// `<think>` backstop (all providers).
// ---------------------------------------------------------------------------

/// Single-commit-message system prompt (shared by every provider).
pub(super) const SYSTEM_PROMPT: &str = "\
Analyze this git diff and generate a concise, conventional commit message.
Use format: <type>(<scope>): <description>
Types: feat, fix, docs, style, refactor, test, chore
Keep the first line under 72 characters.
Add a blank line and bullet points for details if there are multiple significant changes.
Do NOT include any explanation - output ONLY the commit message.";

/// Grouping-plan system prompt (shared by every provider; the structured-output
/// schema enforces the shape, so the prompt carries only the grouping rules).
pub(super) const GROUPING_SYSTEM_PROMPT: &str = "\
Analyze these git changes. Group related files into logical commits by semantic relevance.

Rules:
- Every file from the file list must appear in exactly one group.
- Prefer fewer groups (1-3) unless changes are truly unrelated.
- commit_message: a full conventional-commit message for groups[0] ONLY; null for every other group.
- Conventional format <type>(<scope>): <description>, first line under 72 chars; add a blank line
  and bullet points for details when there are multiple significant changes.
- For renamed files, use the NEW path in your file list.
- summary: a one-line description of each group.";

/// The grouping-plan user content (shared by every provider's plan call).
pub(super) fn grouping_user_content(ctx: &GroupingContext) -> String {
    format!(
        "Changed files (JSON array of exact paths - group by these):\n{}\n\n\
         Git status (JSON array of \"XY path\"):\n{}\n\nDiff stats:\n{}\n\nFull diff:\n{}",
        ctx.file_list, ctx.status, ctx.stat, ctx.body
    )
}

/// The single-message user content (shared by every provider's message call).
pub(super) fn message_user_content(diff: &GatheredDiff) -> String {
    format!("Diff stats:\n{}\n\nFull diff:\n{}", diff.stat, diff.body)
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

/// Extract the first choice's message content from an OpenAI-compatible
/// chat-completions body (`<think>` stripped, trimmed). Empty content yields an
/// empty string; the caller decides whether empty is an error.
fn extract_openai_content(provider: &'static str, raw: &str) -> Result<String, ProviderError> {
    let parsed: ChatResponse = serde_json::from_str(raw)
        .map_err(|e| ProviderError::new(provider, ErrorKind::Deserialize(e.to_string())))?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();
    Ok(strip_think(&content).trim().to_string())
}

/// Remove any `<think>...</think>` spans (reasoning models that only hide rather
/// than disable CoT, FR-17/FR-20). Drops an unterminated trailing `<think>` too.
/// The universal backstop applied to every provider's response.
fn strip_think(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_parse_canonical_alias_and_case() {
        // canonical names
        assert_eq!(ProviderId::parse("groq"), Some(ProviderId::Groq));
        assert_eq!(ProviderId::parse("google"), Some(ProviderId::Google));
        assert_eq!(ProviderId::parse("openai"), Some(ProviderId::Openai));
        assert_eq!(ProviderId::parse("anthropic"), Some(ProviderId::Anthropic));
        // alias gemini -> Google
        assert_eq!(ProviderId::parse("gemini"), Some(ProviderId::Google));
        // case- and whitespace-insensitive
        assert_eq!(ProviderId::parse("GOOGLE"), Some(ProviderId::Google));
        assert_eq!(ProviderId::parse("  google "), Some(ProviderId::Google));
        assert_eq!(ProviderId::parse("ANTHROPIC"), Some(ProviderId::Anthropic));
        // unknown
        assert_eq!(ProviderId::parse("foo"), None);
    }

    #[test]
    fn pick_provider_id_precedence() {
        // flag wins over env
        assert_eq!(
            pick_provider_id(Some(ProviderId::Openai), Some("google")).unwrap(),
            ProviderId::Openai
        );
        // env when no flag
        assert_eq!(
            pick_provider_id(None, Some("google")).unwrap(),
            ProviderId::Google
        );
        // default groq when neither
        assert_eq!(pick_provider_id(None, None).unwrap(), ProviderId::Groq);
        // empty/whitespace env -> default (not an error)
        assert_eq!(pick_provider_id(None, Some("")).unwrap(), ProviderId::Groq);
        assert_eq!(
            pick_provider_id(None, Some("   ")).unwrap(),
            ProviderId::Groq
        );
    }

    #[test]
    fn pick_provider_id_unknown_is_config_error() {
        let err = pick_provider_id(None, Some("bogus")).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Config(_)));
        assert!(err.to_string().contains("bogus"));
        assert!(err.to_string().contains("groq"));
    }

    #[test]
    fn pick_model_precedence() {
        // flag wins
        assert_eq!(
            pick_model(Some("m-flag"), &[Some("m-env".to_string())], "m-def"),
            "m-flag"
        );
        // env when no flag; first non-empty env wins (primary before alias)
        assert_eq!(
            pick_model(None, &[None, Some("alias".to_string())], "m-def"),
            "alias"
        );
        assert_eq!(
            pick_model(
                None,
                &[Some("primary".to_string()), Some("alias".to_string())],
                "m-def"
            ),
            "primary"
        );
        // default when nothing
        assert_eq!(pick_model(None, &[None], "m-def"), "m-def");
    }

    #[test]
    fn pick_model_empty_flag_and_env_fall_through() {
        // empty/whitespace --model is not a literal model id (round-2 pt / P1.5)
        assert_eq!(pick_model(Some("   "), &[None], "m-def"), "m-def");
        // empty env is skipped
        assert_eq!(
            pick_model(
                None,
                &[Some("  ".to_string()), Some("real".to_string())],
                "m-def"
            ),
            "real"
        );
    }

    #[test]
    fn provider_defaults_and_tokens() {
        assert_eq!(ProviderId::Groq.default_model(), "openai/gpt-oss-120b");
        assert_eq!(ProviderId::Google.default_model(), "gemini-3.1-flash-lite");
        assert_eq!(ProviderId::Openai.default_model(), "gpt-4o-mini-2024-07-18");
        assert_eq!(ProviderId::Anthropic.default_model(), "claude-haiku-4-5");
        // Google reads both gemini + google model envs (primary first)
        assert_eq!(
            ProviderId::Google.model_env_vars(),
            &["GCM_GEMINI_MODEL", "GCM_GOOGLE_MODEL"]
        );
        assert_eq!(
            ProviderId::Anthropic.model_env_vars(),
            &["GCM_ANTHROPIC_MODEL"]
        );
    }

    #[test]
    fn error_display_names_provider_and_env_var() {
        let mk = ProviderError::new(
            "Google",
            ErrorKind::MissingKey {
                env_var: "GEMINI_API_KEY",
            },
        );
        assert!(mk.to_string().contains("Google"));
        assert!(mk.to_string().contains("GEMINI_API_KEY"));
        let auth = ProviderError::new(
            "OpenAI",
            ErrorKind::Auth {
                status: 401,
                env_var: "OPENAI_API_KEY",
            },
        );
        assert!(auth.to_string().contains("OpenAI"));
        assert!(auth.to_string().contains("OPENAI_API_KEY"));
        assert!(auth.to_string().contains("401"));
    }

    #[test]
    fn error_display_variants_distinct_and_nonempty() {
        use std::collections::HashSet;
        let msgs: Vec<String> = vec![
            ProviderError::new("Groq", ErrorKind::RateLimit { retry_after: None }).to_string(),
            ProviderError::new("Groq", ErrorKind::BadRequest { detail: None }).to_string(),
            ProviderError::new("Groq", ErrorKind::Server(500)).to_string(),
            ProviderError::new("Groq", ErrorKind::Timeout).to_string(),
            ProviderError::new("Groq", ErrorKind::EmptyResponse).to_string(),
            ProviderError::new("Groq", ErrorKind::Deserialize("x".to_string())).to_string(),
        ];
        assert!(msgs.iter().all(|m| !m.is_empty()));
        let set: HashSet<&String> = msgs.iter().collect();
        assert_eq!(set.len(), 6, "all six messages must be distinct");
    }

    #[test]
    fn is_retryable_only_ratelimit_and_server() {
        assert!(is_retryable(&ErrorKind::RateLimit { retry_after: None }));
        assert!(is_retryable(&ErrorKind::Server(500)));
        assert!(is_retryable(&ErrorKind::Server(504)));
        for k in [
            ErrorKind::BadRequest { detail: None },
            ErrorKind::Auth {
                status: 401,
                env_var: "K",
            },
            ErrorKind::Timeout,
            ErrorKind::Transport("x".to_string()),
            ErrorKind::EmptyResponse,
            ErrorKind::Deserialize("x".to_string()),
            ErrorKind::MissingKey { env_var: "K" },
            ErrorKind::Http(418),
            ErrorKind::Config("x".to_string()),
        ] {
            assert!(!is_retryable(&k), "{k:?} must not be retryable");
        }
    }

    #[test]
    fn strips_think_block() {
        assert_eq!(
            strip_think("<think>reasoning</think>feat: add thing").trim(),
            "feat: add thing"
        );
        assert_eq!(
            strip_think("docs: x\n<think>oops never closed").trim(),
            "docs: x"
        );
        assert_eq!(strip_think("chore: clean"), "chore: clean");
    }

    #[test]
    fn extract_openai_content_strips_think_and_trims() {
        let raw = r#"{"choices":[{"message":{"content":"<think>hmm</think>  feat: a  "}}]}"#;
        assert_eq!(extract_openai_content("Groq", raw).unwrap(), "feat: a");
        // no choices -> empty string (caller maps to EmptyResponse)
        let empty = r#"{"choices":[]}"#;
        assert_eq!(extract_openai_content("Groq", empty).unwrap(), "");
    }
}
