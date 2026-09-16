//! Shared blocking HTTP transport for every provider (ADR-001 Decision 2): one
//! POST attempt + the bounded-backoff retry engine moved here from CLO-488's
//! `groq.rs`, retyped to the provider-agnostic [`ProviderError`]. Pure
//! classification/policy helpers are unit-tested without a network.

#[cfg(feature = "cli")]
use std::io::Read;
#[cfg(feature = "cli")]
use std::time::Duration;

use serde_json::Value;

#[cfg(feature = "cli")]
use super::identity::{env_u64, is_retryable, retry_after_hint, ErrorKind, ProviderError};

/// Default client timeout. Bumped 30 -> 60s (CLO-489 round-2 review pt 2):
/// reasoning models / large diffs routinely take 45-90s to first token, and a
/// 30s global timeout reliably killed them. Override: `GCM_HTTP_TIMEOUT_SECS`.
#[cfg(feature = "cli")]
const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// Cap on the error-response body read for the `BadRequest` detail (CLO-488): a
/// non-2xx can be a large HTML error page, so never read it unbounded.
#[cfg(feature = "cli")]
const MAX_ERROR_BODY_BYTES: u64 = 4096;
/// Retry budget defaults (FR-22). Overridable via `GCM_RETRY_MAX` /
/// `GCM_RETRY_BASE_MS` / `GCM_RETRY_MAX_MS`.
#[cfg(feature = "cli")]
const DEFAULT_MAX_RETRIES: u32 = 3;
#[cfg(feature = "cli")]
const DEFAULT_RETRY_BASE: Duration = Duration::from_millis(500);
#[cfg(feature = "cli")]
const DEFAULT_RETRY_MAX: Duration = Duration::from_secs(8);
/// Short timeout for the interactive model-list fetch (CLO-516): the `gcm provider`
/// wizard spinner must not hang on a flaky network - one light retry then fall back
/// to the static list. Deliberately separate from the 60s generation timeout.
#[cfg(feature = "cli")]
const MODEL_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// The generation-call budget in seconds: `GCM_HTTP_TIMEOUT_SECS` when set to a
/// positive value, else [`DEFAULT_TIMEOUT_SECS`]. Public so the CLI can name the
/// budget that actually applied when a call times out (CLO-798) instead of
/// duplicating the default. Note this is **not** the budget for
/// [`get_json`], which uses the fixed [`MODEL_FETCH_TIMEOUT`].
#[cfg(feature = "cli")]
pub fn timeout_secs() -> u64 {
    env_u64("GCM_HTTP_TIMEOUT_SECS")
        .filter(|&v| v > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
}

/// The model-list discovery budget in seconds (CLO-798): fixed, not overridable,
/// and deliberately far shorter than [`timeout_secs`].
#[cfg(feature = "cli")]
pub fn model_fetch_timeout_secs() -> u64 {
    MODEL_FETCH_TIMEOUT.as_secs()
}

/// Marker prepended to a `BadRequest` detail when the body is a
/// context-window rejection rather than a malformed request (CLO-798).
///
/// `ErrorKind` is public library surface and cannot grow a variant, and the
/// detail is the only channel from the transport to the CLI. Detection happens
/// here, over the full body, because [`bad_request_detail`] keeps only
/// `error.message` and truncates it - a signal carried in a sibling `code`, or
/// sitting past the truncation point, would otherwise be lost before the CLI
/// ever sees it. The CLI strips this prefix when composing user-facing prose.
pub const CONTEXT_WINDOW_MARKER: &str = "context_window: ";

/// Whether a 400 body is a context-window rejection (CLO-798). Pure.
///
/// Matching is deliberately anchored. A bare "exceeds the maximum" is **not**
/// enough: providers use that phrasing for unrelated limits, and a false
/// positive would tell the user to shrink a diff that is not the problem. Each
/// arm therefore requires a token- or context-specific anchor.
pub fn is_context_window_body(body: &str) -> bool {
    let b = body.to_lowercase();
    // OpenAI / Groq / most OpenAI-compatible backends: a machine code, in
    // `error.code` or `error.type`, which survives regardless of message length.
    if b.contains("context_length_exceeded") || b.contains("string_above_max_length") {
        return true;
    }
    // Groq / OpenAI prose form.
    if b.contains("reduce the length of the messages")
        || b.contains("please reduce the length")
        || b.contains("request too large")
    {
        return true;
    }
    // Anthropic.
    if b.contains("prompt is too long") {
        return true;
    }
    // Gemini / Vertex: require a token-count anchor next to the "exceeds"
    // phrasing, never "exceeds the maximum" on its own.
    // `max_tokens` is deliberately absent: it names the *output* budget, and
    // "max_tokens exceeds the model limit" is an unrelated 400 that shrinking the
    // prompt would not fix.
    let token_anchor = b.contains("input token count")
        || b.contains("token count")
        || b.contains("context length")
        || b.contains("context window");
    if token_anchor && (b.contains("exceed") || b.contains("too many") || b.contains("too large")) {
        return true;
    }
    false
}

/// One provider HTTP request (CLO-489 round-2 review pt 5): `auth` is an optional
/// `(header_name, header_value)` pair passed straight to `ureq` - Groq/OpenAI send
/// `Some(("Authorization", "Bearer <key>"))`, Gemini `Some(("x-goog-api-key", key))`,
/// and the local Ollama provider (CLO-495) sends `None` (no key, no auth header).
pub struct HttpRequest<'a> {
    pub provider: &'static str,
    /// API-key env var, surfaced in an `Auth` (401/403) error message (FR-18).
    /// Meaningful only when `auth` is `Some`; a no-auth backend passes `""`.
    pub auth_env_var: &'static str,
    pub endpoint: String,
    pub auth: Option<(&'static str, String)>,
    /// Additional headers beyond auth + Content-Type (e.g. Anthropic's
    /// `anthropic-version`). Sent after the auth header, before `.send()`.
    pub extra_headers: Vec<(&'static str, String)>,
    pub payload: &'a Value,
}

/// POST a JSON payload and return the raw 2xx body, retrying transient failures
/// (429/5xx) with bounded backoff (FR-22). Response parsing is the caller's
/// concern and is not retried.
#[cfg(feature = "cli")]
pub fn post_json(req: &HttpRequest) -> Result<String, ProviderError> {
    let cfg = RetryConfig::from_env();
    retry_with(&cfg, std::thread::sleep, || send_once(req))
}

/// A model-list discovery GET (CLO-516): like [`HttpRequest`] but no payload.
pub struct HttpGet {
    pub provider: &'static str,
    /// API-key env var, surfaced in an `Auth` (401/403) error; `""` for no-auth.
    pub auth_env_var: &'static str,
    pub endpoint: String,
    pub auth: Option<(&'static str, String)>,
    pub extra_headers: Vec<(&'static str, String)>,
}

/// GET a JSON body for model-list discovery. Short timeout + a single light retry
/// on transient failures so the wizard spinner can't hang; the caller falls back
/// to a static list on any `Err`.
#[cfg(feature = "cli")]
pub fn get_json(req: &HttpGet) -> Result<String, ProviderError> {
    let cfg = RetryConfig {
        max_retries: 1,
        base: Duration::from_millis(200),
        max: Duration::from_secs(2),
    };
    retry_with(&cfg, std::thread::sleep, || get_once(req))
}

/// One GET attempt (mirrors [`send_once`] but with no request body and the short
/// [`MODEL_FETCH_TIMEOUT`]). Non-2xx is classified into a typed [`ErrorKind`].
#[cfg(feature = "cli")]
fn get_once(req: &HttpGet) -> Result<String, ProviderError> {
    let provider = req.provider;
    let wrap = |kind| ProviderError { provider, kind };

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(MODEL_FETCH_TIMEOUT))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut builder = agent.get(&req.endpoint);
    if let Some((name, value)) = req.auth.as_ref() {
        builder = builder.header(*name, value.as_str());
    }
    for (name, value) in &req.extra_headers {
        builder = builder.header(*name, value.as_str());
    }
    let mut response = builder.call().map_err(|e| wrap(map_ureq_error(e)))?;

    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return response
            .body_mut()
            .read_to_string()
            .map_err(|e| wrap(ErrorKind::Transport(e.to_string())));
    }
    let retry_after = parse_retry_after(
        response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
    );
    let mut buf = Vec::new();
    let _ = response
        .body_mut()
        .as_reader()
        .take(MAX_ERROR_BODY_BYTES)
        .read_to_end(&mut buf);
    let err_body = String::from_utf8_lossy(&buf);
    let kind = classify_status(
        status,
        retry_after,
        bad_request_detail(&err_body),
        req.auth.as_ref().map(|_| req.auth_env_var),
    );
    crate::debug_log!("{provider} model-list response error: {kind:?}");
    Err(wrap(kind))
}

/// One HTTP attempt. Non-2xx responses are inspected (status + `Retry-After` +
/// a capped error body) and classified into a typed [`ErrorKind`] (FR-21);
/// pre-response transport failures map via [`map_ureq_error`].
#[cfg(feature = "cli")]
fn send_once(req: &HttpRequest) -> Result<String, ProviderError> {
    let provider = req.provider;
    let wrap = |kind| ProviderError { provider, kind };

    let body = serde_json::to_string(req.payload)
        .map_err(|e| wrap(ErrorKind::Deserialize(e.to_string())))?;
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs())))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut builder = agent
        .post(&req.endpoint)
        .header("Content-Type", "application/json");
    // No-auth backends (Ollama) send no auth header; everyone else sends one.
    if let Some((name, value)) = req.auth.as_ref() {
        builder = builder.header(*name, value.as_str());
    }
    // Additional provider headers (e.g. Anthropic's `anthropic-version`).
    for (name, value) in &req.extra_headers {
        builder = builder.header(*name, value.as_str());
    }
    let mut response = builder
        .send(body.as_str())
        .map_err(|e| wrap(map_ureq_error(e)))?;

    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return response
            .body_mut()
            .read_to_string()
            .map_err(|e| wrap(ErrorKind::Transport(e.to_string())));
    }
    // Non-2xx: capture the case-insensitive Retry-After hint + a size-capped
    // error body (std `Take` so a hit cap truncates cleanly), then classify.
    let retry_after = parse_retry_after(
        response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
    );
    let mut buf = Vec::new();
    let _ = response
        .body_mut()
        .as_reader()
        .take(MAX_ERROR_BODY_BYTES)
        .read_to_end(&mut buf);
    let err_body = String::from_utf8_lossy(&buf);
    // Only surface the auth env var when the request actually carried auth - a
    // no-auth backend (Ollama, `auth: None`) must never synthesize an `Auth`
    // error naming an empty placeholder env var (CLO-495 validation).
    let kind = classify_status(
        status,
        retry_after,
        bad_request_detail(&err_body),
        req.auth.as_ref().map(|_| req.auth_env_var),
    );
    crate::debug_log!("{provider} response error: {kind:?}");
    Err(wrap(kind))
}

#[cfg(feature = "cli")]
/// Classify a non-2xx HTTP status into a typed [`ErrorKind`] (pure; unit-tested).
/// 504 (Gateway Timeout) is a `Server` error, NOT the client-side `Timeout`.
fn classify_status(
    status: u16,
    retry_after: Option<Duration>,
    detail: Option<String>,
    auth_env_var: Option<&'static str>,
) -> ErrorKind {
    match status {
        400 => ErrorKind::BadRequest { detail },
        // 401/403 mean "bad key" only for a backend that sends one; a no-auth
        // backend (`auth_env_var: None`, e.g. Ollama behind a proxy) treats them
        // as a generic HTTP error rather than naming a nonexistent key var.
        401 | 403 => match auth_env_var {
            Some(env_var) => ErrorKind::Auth { status, env_var },
            None => ErrorKind::Http(status),
        },
        429 => ErrorKind::RateLimit { retry_after },
        500..=599 => ErrorKind::Server(status),
        _ => ErrorKind::Http(status),
    }
}

#[cfg(feature = "cli")]
/// Parse a `Retry-After` header value (integer seconds only; HTTP-date or
/// unparseable/empty -> `None`).
fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

/// Pull an actionable detail from a 400/blocked body: JSON `error.message` when
/// present, else the raw body trimmed/truncated to 200 chars; `None` if empty.
///
/// When the **full** body is a context-window rejection the detail is prefixed
/// with [`CONTEXT_WINDOW_MARKER`] (CLO-798). Detection has to happen here rather
/// than downstream: `error.message` alone loses a sibling `error.code`, and the
/// 200-char truncation can cut the phrase off entirely, so by the time the CLI
/// receives the detail the evidence may be gone.
pub fn bad_request_detail(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    let detail = serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .map(|m| truncate(m, 200))
        })
        .unwrap_or_else(|| truncate(trimmed, 200));
    if is_context_window_body(body) {
        return Some(format!("{CONTEXT_WINDOW_MARKER}{detail}"));
    }
    Some(detail)
}

/// Truncate to at most `max` characters (char-safe).
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// Bounded exponential-backoff config for transient failures (FR-22).
#[cfg(feature = "cli")]
struct RetryConfig {
    max_retries: u32,
    base: Duration,
    max: Duration,
}

#[cfg(feature = "cli")]
impl RetryConfig {
    fn from_env() -> Self {
        RetryConfig {
            max_retries: env_u64("GCM_RETRY_MAX")
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(DEFAULT_MAX_RETRIES),
            base: env_u64("GCM_RETRY_BASE_MS")
                .map(Duration::from_millis)
                .unwrap_or(DEFAULT_RETRY_BASE),
            max: env_u64("GCM_RETRY_MAX_MS")
                .map(Duration::from_millis)
                .unwrap_or(DEFAULT_RETRY_MAX),
        }
    }
}

/// Backoff before the next attempt: honor a `Retry-After` hint (capped at
/// `cfg.max`), else exponential `base * 2^attempt` capped at `cfg.max`.
#[cfg(feature = "cli")]
fn backoff_delay(attempt: u32, hint: Option<Duration>, cfg: &RetryConfig) -> Duration {
    if let Some(d) = hint {
        return d.min(cfg.max);
    }
    let factor = 2u32.saturating_pow(attempt.min(16));
    cfg.base.saturating_mul(factor).min(cfg.max)
}

/// Short human reason for the default-on retry notice (CLO-798). Pure.
/// [`is_retryable`] admits only rate limits and 5xx, so the fallback arm is
/// defensive rather than reachable.
#[cfg(feature = "cli")]
fn retry_reason(kind: &ErrorKind) -> String {
    match kind {
        ErrorKind::RateLimit { .. } => "rate limited (HTTP 429)".to_string(),
        ErrorKind::Server(status) => format!("server error (HTTP {status})"),
        _ => "transient failure".to_string(),
    }
}

/// Run `op`, retrying transient failures with bounded backoff. The sleeper is
/// injected (`FnMut`) so tests record delays with no real sleep and no network.
#[cfg(feature = "cli")]
fn retry_with<T>(
    cfg: &RetryConfig,
    mut sleep: impl FnMut(Duration),
    mut op: impl FnMut() -> Result<T, ProviderError>,
) -> Result<T, ProviderError> {
    let mut attempt = 0u32;
    loop {
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if attempt >= cfg.max_retries || !is_retryable(&e.kind) {
                    return Err(e);
                }
                let delay = backoff_delay(attempt, retry_after_hint(&e.kind), cfg);
                // Warn, not debug (CLO-798): four attempts each running to the
                // 60s timeout is ~4 minutes, and this line is the only thing
                // distinguishing that from a hung process.
                crate::warn_log!(
                    "{} {}; attempt {} of {}, retrying in {delay:?}",
                    e.provider,
                    retry_reason(&e.kind),
                    u64::from(attempt) + 1,
                    u64::from(cfg.max_retries) + 1
                );
                crate::debug_log!("{} retry detail: {:?}", e.provider, e.kind);
                sleep(delay);
                attempt += 1;
            }
        }
    }
}

#[cfg(feature = "cli")]
fn map_ureq_error(err: ureq::Error) -> ErrorKind {
    match err {
        ureq::Error::StatusCode(code) => ErrorKind::Http(code),
        ureq::Error::Timeout(_) => ErrorKind::Timeout,
        ureq::Error::HostNotFound => ErrorKind::Transport("host not found".to_string()),
        ureq::Error::Io(e) => ErrorKind::Transport(e.to_string()),
        other => ErrorKind::Transport(other.to_string()),
    }
}

#[cfg(all(test, feature = "cli"))]
mod tests {
    use super::*;

    fn perr(kind: ErrorKind) -> ProviderError {
        ProviderError {
            provider: "Test",
            kind,
        }
    }

    fn cfg(max_retries: u32, base_ms: u64, max_ms: u64) -> RetryConfig {
        RetryConfig {
            max_retries,
            base: Duration::from_millis(base_ms),
            max: Duration::from_millis(max_ms),
        }
    }

    /// CLO-798 AC-5: the detector must fire on every real context-window
    /// rejection and on none of the look-alikes. The two hazards it exists for
    /// are a signal that lives only in a sibling `code`, and one that sits past
    /// the 200-char truncation of `error.message`.
    #[test]
    fn context_window_detector() {
        // Signal in the message (Groq / OpenAI prose).
        assert!(is_context_window_body(
            r#"{"error":{"message":"Please reduce the length of the messages"}}"#
        ));
        // Signal only in a sibling code - the message says nothing useful.
        assert!(is_context_window_body(
            r#"{"error":{"message":"Request too large","code":"context_length_exceeded"}}"#
        ));
        // Signal past character 200 of the message.
        let long = format!(
            r#"{{"error":{{"message":"{}  the prompt is too long for this model"}}}}"#,
            "padding ".repeat(40)
        );
        assert!(long.len() > 200);
        assert!(is_context_window_body(&long));
        // Anthropic.
        assert!(is_context_window_body(
            r#"{"error":{"type":"invalid_request_error","message":"prompt is too long: 250000 tokens"}}"#
        ));
        // Gemini / Vertex, with the token anchor present.
        assert!(is_context_window_body(
            r#"{"error":{"message":"The input token count (1200000) exceeds the maximum allowed"}}"#
        ));

        // Unrelated 400s must not trip it.
        for body in [
            r#"{"error":{"message":"Unsupported parameter: 'response_format'"}}"#,
            r#"{"error":{"message":"model `nope` does not exist","code":"model_not_found"}}"#,
            "<html><body>400 Bad Request</body></html>",
        ] {
            assert!(!is_context_window_body(body), "false positive on {body}");
        }
        // The bare phrase without a token anchor is explicitly NOT enough: other
        // limits are worded the same way and shrinking the diff would not help.
        assert!(!is_context_window_body(
            r#"{"error":{"message":"value exceeds the maximum allowed length"}}"#
        ));
        // An *output* budget rejection is not a context-window rejection. Telling
        // the user to send a smaller diff would not fix it.
        for body in [
            r#"{"error":{"message":"max_tokens: 200000 exceeds the model limit","type":"invalid_request_error"}}"#,
            r#"{"error":{"message":"max_completion_tokens is too large"}}"#,
        ] {
            assert!(!is_context_window_body(body), "false positive on {body}");
        }
    }

    /// The marker has to reach the CLI on the same paths the detector fires on,
    /// and must never appear otherwise.
    #[test]
    fn bad_request_detail_marks_context_window() {
        let marked = bad_request_detail(
            r#"{"error":{"message":"Request too large","code":"context_length_exceeded"}}"#,
        )
        .unwrap();
        assert!(marked.starts_with(CONTEXT_WINDOW_MARKER), "got {marked:?}");
        assert!(marked.ends_with("Request too large"));

        let plain = bad_request_detail(r#"{"error":{"message":"Unsupported parameter"}}"#).unwrap();
        assert!(!plain.contains(CONTEXT_WINDOW_MARKER));
        assert_eq!(plain, "Unsupported parameter");

        assert_eq!(bad_request_detail("   "), None);
    }

    /// CLO-798: the default-on retry notice names what went wrong in words,
    /// not a Debug dump.
    #[test]
    fn retry_reason_is_human() {
        assert_eq!(
            retry_reason(&ErrorKind::RateLimit { retry_after: None }),
            "rate limited (HTTP 429)"
        );
        assert_eq!(
            retry_reason(&ErrorKind::Server(503)),
            "server error (HTTP 503)"
        );
        // Every kind retry_with can actually see is covered above; the rest of
        // the taxonomy still renders rather than panicking.
        assert_eq!(retry_reason(&ErrorKind::Timeout), "transient failure");
    }

    #[test]
    fn classify_status_maps_codes() {
        assert!(matches!(
            classify_status(400, None, None, Some("K")),
            ErrorKind::BadRequest { .. }
        ));
        assert!(matches!(
            classify_status(401, None, None, Some("K")),
            ErrorKind::Auth { status: 401, .. }
        ));
        assert!(matches!(
            classify_status(403, None, None, Some("K")),
            ErrorKind::Auth { status: 403, .. }
        ));
        assert!(matches!(
            classify_status(429, None, None, Some("K")),
            ErrorKind::RateLimit { .. }
        ));
        assert!(matches!(
            classify_status(500, None, None, Some("K")),
            ErrorKind::Server(500)
        ));
        // 504 Gateway Timeout is a Server error, NOT the client-side Timeout.
        assert!(matches!(
            classify_status(504, None, None, Some("K")),
            ErrorKind::Server(504)
        ));
        assert!(matches!(
            classify_status(418, None, None, Some("K")),
            ErrorKind::Http(418)
        ));
    }

    #[test]
    fn classify_status_no_auth_401_403_is_http_not_auth() {
        // CLO-495: a no-auth backend (Ollama, auth_env_var None) must not
        // synthesize an Auth error naming an empty env var on a fronting-proxy
        // 401/403 - it degrades to a generic HTTP error.
        assert!(matches!(
            classify_status(401, None, None, None),
            ErrorKind::Http(401)
        ));
        assert!(matches!(
            classify_status(403, None, None, None),
            ErrorKind::Http(403)
        ));
        // 400/429/5xx are unaffected by the auth-var presence.
        assert!(matches!(
            classify_status(400, None, None, None),
            ErrorKind::BadRequest { .. }
        ));
    }

    #[test]
    fn parse_retry_after_seconds_only() {
        assert_eq!(parse_retry_after(Some("2")), Some(Duration::from_secs(2)));
        assert_eq!(
            parse_retry_after(Some("  5 ")),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn bad_request_detail_prefers_json_then_truncates() {
        assert_eq!(
            bad_request_detail(r#"{"error":{"message":"bad model"}}"#).as_deref(),
            Some("bad model")
        );
        let raw = "x".repeat(500);
        assert!(bad_request_detail(&raw).unwrap().chars().count() <= 200);
        assert_eq!(bad_request_detail("   "), None);
    }

    #[test]
    fn backoff_schedule_doubles_and_caps() {
        let c = cfg(5, 100, 1000);
        assert_eq!(backoff_delay(0, None, &c), Duration::from_millis(100));
        assert_eq!(backoff_delay(1, None, &c), Duration::from_millis(200));
        assert_eq!(backoff_delay(2, None, &c), Duration::from_millis(400));
        assert_eq!(backoff_delay(4, None, &c), Duration::from_millis(1000)); // capped
        assert_eq!(backoff_delay(20, None, &c), Duration::from_millis(1000)); // no overflow
    }

    #[test]
    fn backoff_honors_retry_after_capped() {
        let c = cfg(3, 100, 1000);
        assert_eq!(
            backoff_delay(0, Some(Duration::from_millis(500)), &c),
            Duration::from_millis(500)
        );
        assert_eq!(
            backoff_delay(0, Some(Duration::from_secs(99)), &c),
            Duration::from_millis(1000)
        );
    }

    #[test]
    fn retry_succeeds_after_two_429() {
        let c = cfg(3, 10, 100);
        let mut sleeps = Vec::new();
        let mut results = vec![
            Err(perr(ErrorKind::RateLimit { retry_after: None })),
            Err(perr(ErrorKind::RateLimit { retry_after: None })),
            Ok(42),
        ]
        .into_iter();
        let mut calls = 0;
        let out = retry_with(
            &c,
            |d| sleeps.push(d),
            || {
                calls += 1;
                results.next().unwrap()
            },
        );
        assert_eq!(out.unwrap(), 42);
        assert_eq!(calls, 3);
        assert_eq!(
            sleeps,
            vec![Duration::from_millis(10), Duration::from_millis(20)]
        );
    }

    #[test]
    fn retry_does_not_retry_bad_request() {
        let c = cfg(3, 10, 100);
        let mut sleeps = Vec::new();
        let mut calls = 0;
        let out: Result<i32, ProviderError> = retry_with(
            &c,
            |d| sleeps.push(d),
            || {
                calls += 1;
                Err(perr(ErrorKind::BadRequest { detail: None }))
            },
        );
        assert!(matches!(
            out.unwrap_err().kind,
            ErrorKind::BadRequest { .. }
        ));
        assert_eq!(calls, 1);
        assert!(sleeps.is_empty());
    }

    #[test]
    fn retry_exhausts_on_persistent_5xx() {
        let c = cfg(3, 10, 100);
        let mut sleeps = Vec::new();
        let mut calls = 0;
        let out: Result<i32, ProviderError> = retry_with(
            &c,
            |d| sleeps.push(d),
            || {
                calls += 1;
                Err(perr(ErrorKind::Server(500)))
            },
        );
        assert!(matches!(out.unwrap_err().kind, ErrorKind::Server(500)));
        assert_eq!(calls, 4); // 1 initial + 3 retries
        assert_eq!(sleeps.len(), 3);
    }
}
