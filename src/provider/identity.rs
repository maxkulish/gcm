//! Provider identity types (CLO-596): error taxonomy, provider id, auth method,
//! model source, and pure model resolution. These types live in the library target
//! so other in-org crates can resolve/select providers and fetch model registries
//! without pulling in the CLI or transport stack.
//!
//! Binary-only concerns (the [`Provider`] trait, conflict resolution, the
//! per-provider backends, and `select()`) remain in the binary facade.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Typed, provider-agnostic failure taxonomy (FR-21). Carries the active provider
/// name so [`fmt::Display`] is specific without a separate variant per provider.
#[derive(Debug)]
pub struct ProviderError {
    pub provider: &'static str,
    pub kind: ErrorKind,
}

impl ProviderError {
    pub fn new(provider: &'static str, kind: ErrorKind) -> Self {
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

impl std::error::Error for ProviderError {}

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

/// Which `kind`s are worth retrying (FR-22): only transient 429 / 5xx.
#[allow(dead_code)]
pub(crate) fn is_retryable(kind: &ErrorKind) -> bool {
    matches!(kind, ErrorKind::RateLimit { .. } | ErrorKind::Server(_))
}

/// The server's `Retry-After` hint, when the error carries one (429 only).
#[allow(dead_code)]
pub(crate) fn retry_after_hint(kind: &ErrorKind) -> Option<Duration> {
    match kind {
        ErrorKind::RateLimit { retry_after } => *retry_after,
        _ => None,
    }
}

/// Read a non-empty, parseable `u64` env var, else `None` (shared by submodules).
#[allow(dead_code)]
pub(crate) fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// The selectable providers (FR-12). `--provider` accepts the lower-case names;
/// `google` also accepts the alias `gemini` (its API key is `GEMINI_API_KEY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "clap", value(rename_all = "lower"))]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Groq,
    #[cfg_attr(feature = "clap", value(alias = "gemini"))]
    #[serde(alias = "gemini")]
    Google,
    Openai,
    Anthropic,
    Ollama,
    #[cfg_attr(feature = "clap", value(alias = "google-vertex"))]
    #[serde(alias = "google-vertex")]
    Vertex,
}

/// How a provider authenticates - the axis that used to be inferred from
/// `key_env_var().is_none()` (CLO-537). `KeylessEndpoint` = Ollama (local URL),
/// `KeylessAdc` = Vertex (gcloud ADC token), `ApiKey` = every key-bearing cloud provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    ApiKey,
    KeylessEndpoint,
    KeylessAdc,
}

/// OpenAI supported model family (CLO-545). Kept in identity so the library's
/// model registry can filter/dedupe OpenAI models without depending on the
/// binary-only `openai` backend module. Re-exported by the facade so the
/// backend shares one source of truth.
#[doc(hidden)]
pub const OPENAI_SUPPORTED_MODELS: &[&str] = &["gpt-5.6-terra", "gpt-5.6-luna"];

impl ProviderId {
    /// The provider's API key env var, or `None` for key-free Ollama and Vertex.
    /// Centralizes the per-backend key mapping so config onboarding (CLO-496)
    /// and the backends agree on one source of truth.
    pub fn key_env_var(self) -> Option<&'static str> {
        match self {
            ProviderId::Groq => Some("GROQ_API_KEY"),
            ProviderId::Google => Some("GEMINI_API_KEY"),
            ProviderId::Openai => Some("OPENAI_API_KEY"),
            ProviderId::Anthropic => Some("ANTHROPIC_API_KEY"),
            ProviderId::Ollama => None,
            ProviderId::Vertex => None,
        }
    }

    /// Default model id (ADR-001 Decisions 5/7 + capability matrix).
    pub fn default_model(self) -> &'static str {
        match self {
            ProviderId::Groq => "openai/gpt-oss-120b",
            ProviderId::Google => "gemini-3.5-flash-lite",
            ProviderId::Openai => OPENAI_SUPPORTED_MODELS[0],
            ProviderId::Anthropic => "claude-haiku-4-5",
            // Local, user-pulled model (FR-56; owner default). `:cloud` variants
            // (e.g. deepseek-v4-flash:cloud) work via --model but are NOT zero-egress.
            ProviderId::Ollama => "gemma4:e4b-mlx",
            ProviderId::Vertex => "gemini-3.5-flash-lite",
        }
    }

    /// Per-provider model env vars, in precedence order (primary first). Google
    /// reads both `GCM_GEMINI_MODEL` (primary, matches `GEMINI_API_KEY`) and the
    /// `GCM_GOOGLE_MODEL` alias (round-2 review pt 4).
    pub fn model_env_vars(self) -> &'static [&'static str] {
        match self {
            ProviderId::Groq => &["GCM_GROQ_MODEL"],
            ProviderId::Google => &["GCM_GEMINI_MODEL", "GCM_GOOGLE_MODEL"],
            ProviderId::Openai => &["GCM_OPENAI_MODEL"],
            ProviderId::Anthropic => &["GCM_ANTHROPIC_MODEL"],
            ProviderId::Ollama => &["GCM_OLLAMA_MODEL"],
            ProviderId::Vertex => &["GCM_VERTEX_MODEL"],
        }
    }

    /// Parse a provider name (env), case- and whitespace-insensitive, honoring
    /// the `gemini` → Google and `google-vertex` → Vertex aliases. Clap-free so
    /// the no-default-features library build compiles.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "groq" => Some(Self::Groq),
            "google" | "gemini" => Some(Self::Google),
            "openai" => Some(Self::Openai),
            "anthropic" => Some(Self::Anthropic),
            "ollama" => Some(Self::Ollama),
            "vertex" | "google-vertex" => Some(Self::Vertex),
            _ => None,
        }
    }

    /// Canonical lowercase token (the `--provider` / `GCM_PROVIDER` value, e.g.
    /// `groq`, `google`). Stable identifier used in `gcm status` output (CLO-515).
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Groq => "groq",
            ProviderId::Google => "google",
            ProviderId::Openai => "openai",
            ProviderId::Anthropic => "anthropic",
            ProviderId::Ollama => "ollama",
            ProviderId::Vertex => "vertex",
        }
    }

    /// How this provider authenticates. Replaces `key_env_var().is_none()` as the
    /// "is-Ollama" proxy now that Vertex is a second keyless provider (CLO-537).
    pub fn auth_method(self) -> AuthMethod {
        match self {
            ProviderId::Ollama => AuthMethod::KeylessEndpoint,
            ProviderId::Vertex => AuthMethod::KeylessAdc,
            _ => AuthMethod::ApiKey,
        }
    }
}

#[cfg(test)]
mod auth_method_tests {
    use super::{AuthMethod, ProviderId};

    #[test]
    fn auth_method_returns_correct_variant_for_each_provider() {
        assert_eq!(ProviderId::Groq.auth_method(), AuthMethod::ApiKey);
        assert_eq!(ProviderId::Google.auth_method(), AuthMethod::ApiKey);
        assert_eq!(ProviderId::Openai.auth_method(), AuthMethod::ApiKey);
        assert_eq!(ProviderId::Anthropic.auth_method(), AuthMethod::ApiKey);
        assert_eq!(
            ProviderId::Ollama.auth_method(),
            AuthMethod::KeylessEndpoint
        );
        assert_eq!(ProviderId::Vertex.auth_method(), AuthMethod::KeylessAdc);
    }
}

/// Where a resolved model value came from (CLO-515 source attribution). `Env`
/// carries the winning env-var name, so Google's `GCM_GEMINI_MODEL` >
/// `GCM_GOOGLE_MODEL` precedence is reportable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    Flag,
    Env(&'static str),
    Default,
}

/// Resolve the effective model **and** its source for a provider (CLO-515).
/// Same precedence as the binary's `resolve_model` (flag > per-provider env in
/// order > default), with empty/whitespace flag and env values skipped.
/// `env_lookup` is injected so `gcm status` can attribute without touching
/// process env directly (and unit tests stay hermetic).
pub fn resolve_model_with_source(
    id: ProviderId,
    cli: Option<&str>,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> (String, ModelSource) {
    if let Some(m) = cli.map(str::trim).filter(|m| !m.is_empty()) {
        return (m.to_string(), ModelSource::Flag);
    }
    for &var in id.model_env_vars() {
        if let Some(m) = env_lookup(var)
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            return (m.to_string(), ModelSource::Env(var));
        }
    }
    (id.default_model().to_string(), ModelSource::Default)
}

/// Default Ollama endpoint.
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";
/// Default Ollama port (used by `normalize_host` when the host has no port).
pub const DEFAULT_PORT: &str = "11434";

/// Normalize an `OLLAMA_HOST` value into a base URL. A value with no `://`
/// scheme gets `http://` prepended; if it then carries no explicit port, the
/// Ollama default `:11434` is appended. A value that already has a scheme is
/// taken as-is (no port forced).
pub fn normalize_host(host: &str) -> String {
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

/// Whether an Ollama model routes off-machine through Ollama Cloud rather than
/// running locally. Cloud passthrough models carry a `:cloud` or `-cloud` tag
/// suffix (e.g. `deepseek-v4-flash:cloud`, `nemotron-3-nano:30b-cloud`); the
/// local daemon proxies those requests to a remote backend, so they are NOT
/// zero-egress. Single source of truth for the runtime egress note and the
/// `gcm status` cloud/local tag, so the two never disagree.
pub fn is_cloud_model(model: &str) -> bool {
    let m = model.trim();
    m.ends_with(":cloud") || m.ends_with("-cloud")
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
        assert_eq!(ProviderId::parse("ollama"), Some(ProviderId::Ollama));
        // case-insensitive (CLO-495 review)
        assert_eq!(ProviderId::parse("OLLAMA"), Some(ProviderId::Ollama));
        // alias gemini -> Google
        assert_eq!(ProviderId::parse("gemini"), Some(ProviderId::Google));
        // case- and whitespace-insensitive
        assert_eq!(ProviderId::parse("GOOGLE"), Some(ProviderId::Google));
        assert_eq!(ProviderId::parse("  google "), Some(ProviderId::Google));
        assert_eq!(ProviderId::parse("ANTHROPIC"), Some(ProviderId::Anthropic));
        // vertex alias google-vertex
        assert_eq!(ProviderId::parse("google-vertex"), Some(ProviderId::Vertex));
        assert_eq!(ProviderId::parse("VERTEX"), Some(ProviderId::Vertex));
        // unknown
        assert_eq!(ProviderId::parse("foo"), None);
    }

    #[test]
    fn provider_id_key_env_var_mapping() {
        // CLO-496: each cloud provider maps to its key env var; Ollama/Vertex are key-free.
        assert_eq!(ProviderId::Groq.key_env_var(), Some("GROQ_API_KEY"));
        assert_eq!(ProviderId::Google.key_env_var(), Some("GEMINI_API_KEY"));
        assert_eq!(ProviderId::Openai.key_env_var(), Some("OPENAI_API_KEY"));
        assert_eq!(
            ProviderId::Anthropic.key_env_var(),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(ProviderId::Ollama.key_env_var(), None);
        assert_eq!(ProviderId::Vertex.key_env_var(), None);
    }

    #[test]
    fn provider_id_serde_round_trip_with_alias() {
        // serde renders lowercase canonical names...
        assert_eq!(
            serde_json::to_string(&ProviderId::Google).unwrap(),
            "\"google\""
        );
        assert_eq!(
            serde_json::to_string(&ProviderId::Ollama).unwrap(),
            "\"ollama\""
        );
        // ...and parses both the canonical name and the `gemini` alias to Google.
        assert_eq!(
            serde_json::from_str::<ProviderId>("\"google\"").unwrap(),
            ProviderId::Google
        );
        assert_eq!(
            serde_json::from_str::<ProviderId>("\"gemini\"").unwrap(),
            ProviderId::Google
        );
        // ...and the google-vertex alias to Vertex.
        assert_eq!(
            serde_json::from_str::<ProviderId>("\"google-vertex\"").unwrap(),
            ProviderId::Vertex
        );
        // round-trips for every variant
        for id in [
            ProviderId::Groq,
            ProviderId::Google,
            ProviderId::Openai,
            ProviderId::Anthropic,
            ProviderId::Ollama,
            ProviderId::Vertex,
        ] {
            let s = serde_json::to_string(&id).unwrap();
            assert_eq!(serde_json::from_str::<ProviderId>(&s).unwrap(), id);
        }
    }

    #[test]
    fn resolve_model_with_source_precedence() {
        // flag wins, source Flag
        let (m, s) = resolve_model_with_source(ProviderId::Groq, Some("m-flag"), |_| {
            Some("m-env".to_string())
        });
        assert_eq!(m, "m-flag");
        assert_eq!(s, ModelSource::Flag);

        // env when no flag; for Google the primary (GCM_GEMINI_MODEL) wins over the
        // alias (GCM_GOOGLE_MODEL) and the source names the winning var.
        let (m, s) = resolve_model_with_source(ProviderId::Google, None, |v| match v {
            "GCM_GEMINI_MODEL" => Some("primary".to_string()),
            "GCM_GOOGLE_MODEL" => Some("alias".to_string()),
            _ => None,
        });
        assert_eq!(m, "primary");
        assert_eq!(s, ModelSource::Env("GCM_GEMINI_MODEL"));

        // alias used when primary unset
        let (m, s) = resolve_model_with_source(ProviderId::Google, None, |v| match v {
            "GCM_GOOGLE_MODEL" => Some("alias".to_string()),
            _ => None,
        });
        assert_eq!(m, "alias");
        assert_eq!(s, ModelSource::Env("GCM_GOOGLE_MODEL"));

        // default when nothing set, source Default
        let (m, s) = resolve_model_with_source(ProviderId::Groq, None, |_| None);
        assert_eq!(m, ProviderId::Groq.default_model());
        assert_eq!(s, ModelSource::Default);
    }

    #[test]
    fn resolve_model_with_source_empty_flag_and_env_fall_through() {
        // empty/whitespace --model is not a literal model id (round-2 pt / P1.5)
        let (m, s) = resolve_model_with_source(ProviderId::Groq, Some("   "), |_| None);
        assert_eq!(m, ProviderId::Groq.default_model());
        assert_eq!(s, ModelSource::Default);
        // empty env is skipped, falls to default
        let (m, s) = resolve_model_with_source(ProviderId::Groq, None, |_| Some("  ".to_string()));
        assert_eq!(m, ProviderId::Groq.default_model());
        assert_eq!(s, ModelSource::Default);
    }

    #[test]
    fn provider_defaults_and_tokens() {
        assert_eq!(ProviderId::Groq.default_model(), "openai/gpt-oss-120b");
        assert_eq!(ProviderId::Google.default_model(), "gemini-3.5-flash-lite");
        assert_eq!(ProviderId::Vertex.default_model(), "gemini-3.5-flash-lite");
        assert_eq!(ProviderId::Openai.default_model(), "gpt-5.6-terra");
        assert_eq!(ProviderId::Anthropic.default_model(), "claude-haiku-4-5");
        assert_eq!(ProviderId::Ollama.default_model(), "gemma4:e4b-mlx");
        assert_eq!(ProviderId::Ollama.model_env_vars(), &["GCM_OLLAMA_MODEL"]);
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
    fn is_cloud_model_detects_both_suffixes() {
        // both the `:cloud` and `-cloud` tag forms route off-machine
        assert!(is_cloud_model("deepseek-v4-flash:cloud"));
        assert!(is_cloud_model("nemotron-3-nano:30b-cloud"));
        assert!(is_cloud_model("  gpt-oss:120b-cloud  ")); // trimmed
                                                           // local GGUF models are not cloud
        assert!(!is_cloud_model("gemma4:e4b-mlx"));
        assert!(!is_cloud_model("llama3:8b"));
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
}
