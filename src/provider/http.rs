//! Shared blocking HTTP transport for every provider (ADR-001 Decision 2): one
//! POST attempt + the bounded-backoff retry engine moved here from CLO-488's
//! `groq.rs`, retyped to the provider-agnostic [`ProviderError`]. Pure
//! classification/policy helpers are unit-tested without a network.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use super::{env_u64, is_retryable, retry_after_hint, ErrorKind, ProviderError};

/// Default client timeout. Bumped 30 -> 60s (CLO-489 round-2 review pt 2):
/// reasoning models / large diffs routinely take 45-90s to first token, and a
/// 30s global timeout reliably killed them. Override: `GCM_HTTP_TIMEOUT_SECS`.
const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// Cap on the error-response body read for the `BadRequest` detail (CLO-488): a
/// non-2xx can be a large HTML error page, so never read it unbounded.
const MAX_ERROR_BODY_BYTES: u64 = 4096;
/// Retry budget defaults (FR-22). Overridable via `GCM_RETRY_MAX` /
/// `GCM_RETRY_BASE_MS` / `GCM_RETRY_MAX_MS`.
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_RETRY_BASE: Duration = Duration::from_millis(500);
const DEFAULT_RETRY_MAX: Duration = Duration::from_secs(8);

fn timeout_secs() -> u64 {
    env_u64("GCM_HTTP_TIMEOUT_SECS")
        .filter(|&v| v > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
}

/// One provider HTTP request (CLO-489 round-2 review pt 5): `auth` is an optional
/// `(header_name, header_value)` pair passed straight to `ureq` - Groq/OpenAI send
/// `Some(("Authorization", "Bearer <key>"))`, Gemini `Some(("x-goog-api-key", key))`,
/// and the local Ollama provider (CLO-495) sends `None` (no key, no auth header).
pub(super) struct HttpRequest<'a> {
    pub provider: &'static str,
    /// API-key env var, surfaced in an `Auth` (401/403) error message (FR-18).
    /// Meaningful only when `auth` is `Some`; a no-auth backend passes `""`.
    pub auth_env_var: &'static str,
    pub endpoint: String,
    pub auth: Option<(&'static str, String)>,
    pub payload: &'a Value,
}

/// POST a JSON payload and return the raw 2xx body, retrying transient failures
/// (429/5xx) with bounded backoff (FR-22). Response parsing is the caller's
/// concern and is not retried.
pub(super) fn post_json(req: &HttpRequest) -> Result<String, ProviderError> {
    let cfg = RetryConfig::from_env();
    retry_with(&cfg, std::thread::sleep, || send_once(req))
}

/// One HTTP attempt. Non-2xx responses are inspected (status + `Retry-After` +
/// a capped error body) and classified into a typed [`ErrorKind`] (FR-21);
/// pre-response transport failures map via [`map_ureq_error`].
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
    let kind = classify_status(
        status,
        retry_after,
        bad_request_detail(&err_body),
        req.auth_env_var,
    );
    crate::debug_log!("{provider} response error: {kind:?}");
    Err(wrap(kind))
}

/// Classify a non-2xx HTTP status into a typed [`ErrorKind`] (pure; unit-tested).
/// 504 (Gateway Timeout) is a `Server` error, NOT the client-side `Timeout`.
fn classify_status(
    status: u16,
    retry_after: Option<Duration>,
    detail: Option<String>,
    auth_env_var: &'static str,
) -> ErrorKind {
    match status {
        400 => ErrorKind::BadRequest { detail },
        401 | 403 => ErrorKind::Auth {
            status,
            env_var: auth_env_var,
        },
        429 => ErrorKind::RateLimit { retry_after },
        500..=599 => ErrorKind::Server(status),
        _ => ErrorKind::Http(status),
    }
}

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
pub(super) fn bad_request_detail(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            return Some(truncate(msg, 200));
        }
    }
    Some(truncate(trimmed, 200))
}

/// Truncate to at most `max` characters (char-safe).
pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// Bounded exponential-backoff config for transient failures (FR-22).
struct RetryConfig {
    max_retries: u32,
    base: Duration,
    max: Duration,
}

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
fn backoff_delay(attempt: u32, hint: Option<Duration>, cfg: &RetryConfig) -> Duration {
    if let Some(d) = hint {
        return d.min(cfg.max);
    }
    let factor = 2u32.saturating_pow(attempt.min(16));
    cfg.base.saturating_mul(factor).min(cfg.max)
}

/// Run `op`, retrying transient failures with bounded backoff. The sleeper is
/// injected (`FnMut`) so tests record delays with no real sleep and no network.
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
                crate::debug_log!(
                    "{} attempt {} failed: {:?}; retrying in {delay:?}",
                    e.provider,
                    attempt + 1,
                    e.kind
                );
                sleep(delay);
                attempt += 1;
            }
        }
    }
}

fn map_ureq_error(err: ureq::Error) -> ErrorKind {
    match err {
        ureq::Error::StatusCode(code) => ErrorKind::Http(code),
        ureq::Error::Timeout(_) => ErrorKind::Timeout,
        ureq::Error::HostNotFound => ErrorKind::Transport("host not found".to_string()),
        ureq::Error::Io(e) => ErrorKind::Transport(e.to_string()),
        other => ErrorKind::Transport(other.to_string()),
    }
}

#[cfg(test)]
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

    #[test]
    fn classify_status_maps_codes() {
        assert!(matches!(
            classify_status(400, None, None, "K"),
            ErrorKind::BadRequest { .. }
        ));
        assert!(matches!(
            classify_status(401, None, None, "K"),
            ErrorKind::Auth { status: 401, .. }
        ));
        assert!(matches!(
            classify_status(403, None, None, "K"),
            ErrorKind::Auth { status: 403, .. }
        ));
        assert!(matches!(
            classify_status(429, None, None, "K"),
            ErrorKind::RateLimit { .. }
        ));
        assert!(matches!(
            classify_status(500, None, None, "K"),
            ErrorKind::Server(500)
        ));
        // 504 Gateway Timeout is a Server error, NOT the client-side Timeout.
        assert!(matches!(
            classify_status(504, None, None, "K"),
            ErrorKind::Server(504)
        ));
        assert!(matches!(
            classify_status(418, None, None, "K"),
            ErrorKind::Http(418)
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
