use std::fmt;
use std::io::Read;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::debug;
use crate::diff::{GatheredDiff, GroupingContext};
use crate::plan::Plan;

const DEFAULT_MODEL: &str = "openai/gpt-oss-120b";
const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
const TIMEOUT_SECS: u64 = 30;
/// Cap on the error-response body we read for the `BadRequest` detail: a non-2xx
/// can be a large HTML error page (e.g. a CDN 502/504), so never read it
/// unbounded (CLO-488 review).
const MAX_ERROR_BODY_BYTES: u64 = 4096;
/// Retry budget defaults (FR-22). Up to 4 total attempts; backoff doubles from
/// 500ms, capped at 8s. A CLI waits synchronously, so the bound is deliberately
/// tight. Overridable via `GCM_RETRY_MAX` / `GCM_RETRY_BASE_MS` / `GCM_RETRY_MAX_MS`.
const DEFAULT_MAX_RETRIES: u32 = 3;
const DEFAULT_RETRY_BASE: Duration = Duration::from_millis(500);
const DEFAULT_RETRY_MAX: Duration = Duration::from_secs(8);

const SYSTEM_PROMPT: &str = "\
Analyze this git diff and generate a concise, conventional commit message.
Use format: <type>(<scope>): <description>
Types: feat, fix, docs, style, refactor, test, chore
Keep the first line under 72 characters.
Add a blank line and bullet points for details if there are multiple significant changes.
Do NOT include any explanation - output ONLY the commit message.";

/// System prompt for the grouping plan (CLO-487; adapted from the bash tool,
/// `docs/tmp/git-commit-ai.sh:305-322`). The `response_format` json_schema
/// enforces the output shape, so the prompt carries only the grouping rules.
const GROUPING_SYSTEM_PROMPT: &str = "\
Analyze these git changes. Group related files into logical commits by semantic relevance.

Rules:
- Every file from the file list must appear in exactly one group.
- Prefer fewer groups (1-3) unless changes are truly unrelated.
- commit_message: a full conventional-commit message for groups[0] ONLY; null for every other group.
- Conventional format <type>(<scope>): <description>, first line under 72 chars; add a blank line
  and bullet points for details when there are multiple significant changes.
- For renamed files, use the NEW path in your file list.
- summary: a one-line description of each group.";

/// Typed taxonomy of Groq provider failures (FR-21). Each variant maps to a
/// distinct, actionable [`fmt::Display`] message; [`is_retryable`] decides which
/// are retried with bounded backoff (FR-22). CLO-489 may lift this behind the
/// provider trait.
#[derive(Debug)]
pub enum GroqError {
    /// `GROQ_API_KEY` is unset or blank (config error; fatal, never retried).
    MissingKey,
    /// HTTP 429 rate limit (retryable). `retry_after` carries a parsed
    /// `Retry-After` (seconds) hint when the server sent one.
    RateLimit { retry_after: Option<Duration> },
    /// HTTP 401/403: the API key was rejected (fatal - bad/expired key).
    Auth(u16),
    /// HTTP 400: the request was rejected - an unsupported parameter/model or a
    /// gcm bug (not retried). `detail` carries the provider's error message when
    /// available.
    BadRequest { detail: Option<String> },
    /// HTTP 5xx server error, incl. 502/503/504 Gateway Timeout (retryable).
    Server(u16),
    /// Any other unexpected non-2xx status (not retried).
    Http(u16),
    /// The request timed out client-side after [`TIMEOUT_SECS`] (not retried).
    Timeout,
    /// A connection/transport failure - DNS, refused, reset (not retried).
    Transport(String),
    /// A 2xx response carried no usable content (not retried).
    EmptyResponse,
    /// The response/plan could not be parsed (not retried).
    Deserialize(String),
}

impl fmt::Display for GroqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroqError::MissingKey => write!(
                f,
                "GROQ_API_KEY is not set. Export it (e.g. `export GROQ_API_KEY=...`) and retry."
            ),
            GroqError::RateLimit { .. } => write!(
                f,
                "Groq rate limit reached (HTTP 429); wait a moment and retry, or use a different provider."
            ),
            GroqError::Auth(code) => write!(
                f,
                "Groq rejected the API key (HTTP {code}); check that GROQ_API_KEY is valid and not expired."
            ),
            GroqError::BadRequest { detail: Some(d) } => write!(
                f,
                "Groq rejected the request (HTTP 400): {d}. Likely an unsupported model/parameter or a gcm bug; please report it."
            ),
            GroqError::BadRequest { detail: None } => write!(
                f,
                "Groq rejected the request (HTTP 400). Likely an unsupported model/parameter or a gcm bug; please report it."
            ),
            GroqError::Server(code) => write!(
                f,
                "Groq server error (HTTP {code}); this is usually transient - retry shortly."
            ),
            GroqError::Http(code) => write!(f, "Groq API returned HTTP {code}"),
            GroqError::Timeout => write!(f, "Groq API request timed out after {TIMEOUT_SECS}s"),
            GroqError::Transport(msg) => write!(f, "could not reach the Groq API: {msg}"),
            GroqError::EmptyResponse => write!(f, "Groq returned an empty response"),
            GroqError::Deserialize(msg) => write!(f, "could not parse the Groq response: {msg}"),
        }
    }
}

/// Classify a non-2xx HTTP status into a typed [`GroqError`] (pure; unit-tested).
/// 504 (Gateway Timeout) is a `Server` error, NOT the client-side `Timeout`.
fn classify_status(
    status: u16,
    retry_after: Option<Duration>,
    detail: Option<String>,
) -> GroqError {
    match status {
        400 => GroqError::BadRequest { detail },
        401 | 403 => GroqError::Auth(status),
        429 => GroqError::RateLimit { retry_after },
        500..=599 => GroqError::Server(status),
        _ => GroqError::Http(status),
    }
}

/// Parse a `Retry-After` header value into a duration. Only the integer-seconds
/// form is honored; an HTTP-date (or anything unparseable/empty) yields `None`.
fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

/// Pull an actionable detail from a 400 response body: the JSON `error.message`
/// when present, else the raw body trimmed and truncated to 200 chars; `None`
/// for an empty body.
fn bad_request_detail(body: &str) -> Option<String> {
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

/// Truncate to at most `max` characters (char-safe, never splits a UTF-8 byte).
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

/// Resolve `(api_key, model, base_url)` from the environment - shared by the
/// message (tracer) and plan (grouping) calls.
fn resolve_config() -> Result<(String, String, String), GroqError> {
    let key = std::env::var("GROQ_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or(GroqError::MissingKey)?;
    let model = std::env::var("GCM_GROQ_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let base_url = std::env::var("GCM_GROQ_BASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    Ok((key, model, base_url))
}

/// Retry budget for transient provider failures (FR-22). Bounded so a CLI never
/// hangs; defaults overridable via env for testability and power users.
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

/// Read a non-empty, parseable `u64` env var, else `None`.
fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// Which typed errors are worth retrying (FR-22): only transient ones - a 429
/// rate limit or a 5xx server error. Everything else (400/auth/parse/timeout/
/// transport/empty/missing-key) is terminal for this call.
fn is_retryable(e: &GroqError) -> bool {
    matches!(e, GroqError::RateLimit { .. } | GroqError::Server(_))
}

/// The server's `Retry-After` hint, when the error carries one (429 only).
fn retry_after_hint(e: &GroqError) -> Option<Duration> {
    match e {
        GroqError::RateLimit { retry_after } => *retry_after,
        _ => None,
    }
}

/// Backoff before the next attempt: honor a `Retry-After` hint (capped at
/// `cfg.max`), else exponential `base * 2^attempt` capped at `cfg.max`. The
/// exponent is clamped so the shift can never overflow.
fn backoff_delay(attempt: u32, hint: Option<Duration>, cfg: &RetryConfig) -> Duration {
    if let Some(d) = hint {
        return d.min(cfg.max);
    }
    let factor = 2u32.saturating_pow(attempt.min(16));
    cfg.base.saturating_mul(factor).min(cfg.max)
}

/// Run `op`, retrying transient failures with bounded backoff. The sleeper is
/// injected (`FnMut`) so tests record delays with no real sleep and no network;
/// production passes `std::thread::sleep`.
fn retry_with<T>(
    cfg: &RetryConfig,
    mut sleep: impl FnMut(Duration),
    mut op: impl FnMut() -> Result<T, GroqError>,
) -> Result<T, GroqError> {
    let mut attempt = 0u32;
    loop {
        match op() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if attempt >= cfg.max_retries || !is_retryable(&e) {
                    return Err(e);
                }
                let delay = backoff_delay(attempt, retry_after_hint(&e), cfg);
                debug::log(&format!(
                    "groq attempt {} failed: {e:?}; retrying in {delay:?}",
                    attempt + 1
                ));
                sleep(delay);
                attempt += 1;
            }
        }
    }
}

/// POST a chat-completions payload and return the raw 2xx body, retrying
/// transient failures (429/5xx) with bounded backoff (FR-22). Shared by both
/// calls. The retry loop wraps only the HTTP round-trip; response parsing is the
/// caller's concern and is not retried.
fn send_chat(key: &str, base_url: &str, payload: &Value) -> Result<String, GroqError> {
    let cfg = RetryConfig::from_env();
    retry_with(&cfg, std::thread::sleep, || {
        send_chat_once(key, base_url, payload)
    })
}

/// One HTTP attempt: POST the payload and return the raw 2xx body. Non-2xx
/// responses are inspected (status + `Retry-After` + a capped error body) and
/// classified into a typed [`GroqError`] (FR-21); pre-response transport
/// failures map via [`map_ureq_error`].
fn send_chat_once(key: &str, base_url: &str, payload: &Value) -> Result<String, GroqError> {
    let body = serde_json::to_string(payload).map_err(|e| GroqError::Deserialize(e.to_string()))?;
    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_SECS)))
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent
        .post(&endpoint)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send(body.as_str())
        .map_err(map_ureq_error)?;
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return response
            .body_mut()
            .read_to_string()
            .map_err(|e| GroqError::Transport(e.to_string()));
    }
    // Non-2xx: capture the Retry-After hint (header lookup is case-insensitive)
    // and a size-capped error body for the BadRequest detail, then classify.
    let retry_after = parse_retry_after(
        response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
    );
    // Bounded best-effort read: ureq's `limit().read_to_string()` ERRORS once the
    // cap is hit (dropping the whole body), so use a std `Take` reader, which
    // truncates cleanly, then lossy-decode (a cut mid-UTF-8 must not fail).
    let mut buf = Vec::new();
    let _ = response
        .body_mut()
        .as_reader()
        .take(MAX_ERROR_BODY_BYTES)
        .read_to_end(&mut buf);
    let err_body = String::from_utf8_lossy(&buf);
    let err = classify_status(status, retry_after, bad_request_detail(&err_body));
    debug::log(&format!("groq response error: {err:?}"));
    Err(err)
}

/// Extract the first choice's message content (`<think>` stripped, trimmed).
/// Returns an empty string when there is no content; the caller decides whether
/// empty is an error.
fn first_choice_content(raw: &str) -> Result<String, GroqError> {
    let parsed: ChatResponse =
        serde_json::from_str(raw).map_err(|e| GroqError::Deserialize(e.to_string()))?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();
    Ok(strip_think(&content).trim().to_string())
}

/// Generate a single conventional-commit message for the gathered diff via a
/// direct Groq REST call (FR-10, FR-18). Returns plain text - no JSON plan;
/// this is the single-commit (tracer/fallback) path.
pub fn generate_commit_message(diff: &GatheredDiff) -> Result<String, GroqError> {
    let (key, model, base_url) = resolve_config()?;
    let user_content = format!("Diff stats:\n{}\n\nFull diff:\n{}", diff.stat, diff.body);
    let mut payload = json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_content },
        ],
    });
    apply_reasoning_suppression(&mut payload, &model);
    let raw = send_chat(&key, &base_url, &payload)?;
    let message = first_choice_content(&raw)?;
    if message.is_empty() {
        return Err(GroqError::EmptyResponse);
    }
    Ok(message)
}

/// Request a grouping plan via structured outputs (ADR-001 Decisions 1 & 5):
/// `response_format` json_schema with `strict: true`, deserialized into a typed
/// [`Plan`]. Grouping-path failures fall back to [`generate_commit_message`].
pub fn generate_plan(context: &GroupingContext) -> Result<Plan, GroqError> {
    let (key, model, base_url) = resolve_config()?;
    let payload = build_plan_payload(context, &model);
    let raw = send_chat(&key, &base_url, &payload)?;
    let json = first_choice_content(&raw)?;
    if json.is_empty() {
        return Err(GroqError::EmptyResponse);
    }
    // FR-20 defensive parsing: recover the plan even if the model wrapped/fenced
    // its JSON. A structurally-valid-but-wrong plan still flows through
    // validate_basic in main.rs -> announced fallback.
    crate::plan::parse_defensive(&json).map_err(|e| GroqError::Deserialize(e.to_string()))
}

/// Build the structured-output plan request payload (extracted for testing the
/// contract shape without a network call).
fn build_plan_payload(context: &GroupingContext, model: &str) -> Value {
    let user_content = format!(
        "Changed files (JSON array of exact paths - group by these):\n{}\n\n\
         Git status (JSON array of \"XY path\"):\n{}\n\nDiff stats:\n{}\n\nFull diff:\n{}",
        context.file_list, context.status, context.stat, context.body
    );
    let mut payload = json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": GROUPING_SYSTEM_PROMPT },
            { "role": "user", "content": user_content },
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "commit_plan",
                "strict": true,
                "schema": crate::plan::schema(),
            }
        }
    });
    apply_reasoning_suppression(&mut payload, model);
    payload
}

/// Select reasoning-suppression params by model family so chain-of-thought never
/// reaches the message (ADR-001 #5; capability matrix). `<think>` stripping is the
/// universal backstop applied to the response regardless.
fn apply_reasoning_suppression(payload: &mut serde_json::Value, model: &str) {
    let obj = payload.as_object_mut().expect("payload is a JSON object");
    if model.contains("qwen") {
        obj.insert("reasoning_effort".into(), json!("none"));
    } else if model.contains("gpt-oss") {
        obj.insert("include_reasoning".into(), json!(false));
    }
}

fn map_ureq_error(err: ureq::Error) -> GroqError {
    match err {
        ureq::Error::StatusCode(code) => GroqError::Http(code),
        ureq::Error::Timeout(_) => GroqError::Timeout,
        ureq::Error::HostNotFound => GroqError::Transport("host not found".to_string()),
        ureq::Error::Io(e) => GroqError::Transport(e.to_string()),
        other => GroqError::Transport(other.to_string()),
    }
}

/// Remove any `<think>...</think>` spans (reasoning models that only hide rather
/// than disable CoT). Drops an unterminated trailing `<think>` as well.
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
    fn strips_think_block() {
        let s = "<think>reasoning here</think>feat: add thing";
        assert_eq!(strip_think(s).trim(), "feat: add thing");
    }

    #[test]
    fn strips_multiple_think_blocks() {
        let s = "<think>a</think>fix: x\n<think>b</think>";
        assert_eq!(strip_think(s).trim(), "fix: x");
    }

    #[test]
    fn drops_unterminated_think() {
        let s = "docs: update\n<think>oops never closed";
        assert_eq!(strip_think(s).trim(), "docs: update");
    }

    #[test]
    fn leaves_clean_message_untouched() {
        let s = "chore: bump deps";
        assert_eq!(strip_think(s), "chore: bump deps");
    }

    #[test]
    fn gpt_oss_gets_include_reasoning_false() {
        let mut p = json!({ "model": "openai/gpt-oss-120b" });
        apply_reasoning_suppression(&mut p, "openai/gpt-oss-120b");
        assert_eq!(p["include_reasoning"], json!(false));
        assert!(p.get("reasoning_effort").is_none());
    }

    #[test]
    fn qwen_gets_reasoning_effort_none() {
        let mut p = json!({ "model": "qwen/qwen3.6-27b" });
        apply_reasoning_suppression(&mut p, "qwen/qwen3.6-27b");
        assert_eq!(p["reasoning_effort"], json!("none"));
        assert!(p.get("include_reasoning").is_none());
    }

    #[test]
    fn plan_payload_requests_strict_structured_output() {
        let ctx = GroupingContext {
            file_list: "a.rs\nb.md".to_string(),
            status: " M a.rs\n?? b.md".to_string(),
            stat: "2 files changed".to_string(),
            body: "diff --git a/a.rs b/a.rs".to_string(),
        };
        let p = build_plan_payload(&ctx, "openai/gpt-oss-120b");

        let rf = &p["response_format"];
        assert_eq!(rf["type"], json!("json_schema"));
        assert_eq!(rf["json_schema"]["name"], json!("commit_plan"));
        assert_eq!(rf["json_schema"]["strict"], json!(true));
        // the embedded schema is the typed Plan contract
        assert!(rf["json_schema"]["schema"]["properties"]["groups"].is_object());
        // reasoning suppression carries over for gpt-oss
        assert_eq!(p["include_reasoning"], json!(false));
        // the user message carries the grouping inputs
        let user = p["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("Changed files"));
        assert!(user.contains("a.rs"));
        assert!(user.contains("Git status"));
        assert!(user.contains("diff --git"));
    }

    #[test]
    fn classify_status_maps_codes() {
        assert!(matches!(
            classify_status(400, None, None),
            GroqError::BadRequest { .. }
        ));
        assert!(matches!(
            classify_status(401, None, None),
            GroqError::Auth(401)
        ));
        assert!(matches!(
            classify_status(403, None, None),
            GroqError::Auth(403)
        ));
        assert!(matches!(
            classify_status(429, None, None),
            GroqError::RateLimit { .. }
        ));
        assert!(matches!(
            classify_status(500, None, None),
            GroqError::Server(500)
        ));
        assert!(matches!(
            classify_status(503, None, None),
            GroqError::Server(503)
        ));
        // 504 Gateway Timeout is a Server error, NOT the client-side Timeout (review point 4).
        assert!(matches!(
            classify_status(504, None, None),
            GroqError::Server(504)
        ));
        assert!(matches!(
            classify_status(418, None, None),
            GroqError::Http(418)
        ));
    }

    #[test]
    fn classify_status_carries_retry_after_and_detail() {
        match classify_status(429, Some(Duration::from_secs(2)), None) {
            GroqError::RateLimit { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(2)))
            }
            other => panic!("expected RateLimit, got {other:?}"),
        }
        match classify_status(400, None, Some("bad model".to_string())) {
            GroqError::BadRequest { detail } => assert_eq!(detail.as_deref(), Some("bad model")),
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn parse_retry_after_seconds_date_absent() {
        assert_eq!(parse_retry_after(Some("2")), Some(Duration::from_secs(2)));
        assert_eq!(
            parse_retry_after(Some("  5 ")),
            Some(Duration::from_secs(5))
        );
        assert_eq!(parse_retry_after(Some("0")), Some(Duration::from_secs(0)));
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn bad_request_detail_prefers_json_error_message() {
        assert_eq!(
            bad_request_detail(r#"{"error":{"message":"bad model"}}"#).as_deref(),
            Some("bad model")
        );
    }

    #[test]
    fn bad_request_detail_truncates_raw_non_json() {
        let raw = "x".repeat(500);
        let d = bad_request_detail(&raw).unwrap();
        assert!(
            d.chars().count() <= 200,
            "detail should be capped at 200 chars"
        );
    }

    #[test]
    fn bad_request_detail_none_for_empty() {
        assert_eq!(bad_request_detail(""), None);
        assert_eq!(bad_request_detail("   "), None);
    }

    #[test]
    fn display_messages_are_distinct_and_nonempty() {
        use std::collections::HashSet;
        let msgs: Vec<String> = vec![
            GroqError::RateLimit { retry_after: None }.to_string(),
            GroqError::BadRequest { detail: None }.to_string(),
            GroqError::Auth(401).to_string(),
            GroqError::Server(500).to_string(),
            GroqError::Timeout.to_string(),
            GroqError::Deserialize("x".to_string()).to_string(),
        ];
        assert!(msgs.iter().all(|m| !m.is_empty()));
        let set: HashSet<&String> = msgs.iter().collect();
        assert_eq!(
            set.len(),
            6,
            "all six core variant messages must be distinct"
        );
    }

    #[test]
    fn bad_request_display_includes_detail_and_code() {
        let m = GroqError::BadRequest {
            detail: Some("unsupported response_format".to_string()),
        }
        .to_string();
        assert!(m.contains("unsupported response_format"));
        assert!(m.contains("400"));
    }

    fn cfg(max_retries: u32, base_ms: u64, max_ms: u64) -> RetryConfig {
        RetryConfig {
            max_retries,
            base: Duration::from_millis(base_ms),
            max: Duration::from_millis(max_ms),
        }
    }

    #[test]
    fn is_retryable_only_ratelimit_and_server() {
        assert!(is_retryable(&GroqError::RateLimit { retry_after: None }));
        assert!(is_retryable(&GroqError::Server(500)));
        assert!(is_retryable(&GroqError::Server(504)));
        for e in [
            GroqError::BadRequest { detail: None },
            GroqError::Auth(401),
            GroqError::Timeout,
            GroqError::Transport("x".to_string()),
            GroqError::EmptyResponse,
            GroqError::Deserialize("x".to_string()),
            GroqError::MissingKey,
            GroqError::Http(418),
        ] {
            assert!(!is_retryable(&e), "{e:?} must not be retryable");
        }
    }

    #[test]
    fn retry_after_hint_only_from_ratelimit() {
        assert_eq!(
            retry_after_hint(&GroqError::RateLimit {
                retry_after: Some(Duration::from_secs(3))
            }),
            Some(Duration::from_secs(3))
        );
        assert_eq!(
            retry_after_hint(&GroqError::RateLimit { retry_after: None }),
            None
        );
        assert_eq!(retry_after_hint(&GroqError::Server(503)), None);
    }

    #[test]
    fn backoff_schedule_doubles_and_caps() {
        let c = cfg(5, 100, 1000);
        assert_eq!(backoff_delay(0, None, &c), Duration::from_millis(100));
        assert_eq!(backoff_delay(1, None, &c), Duration::from_millis(200));
        assert_eq!(backoff_delay(2, None, &c), Duration::from_millis(400));
        assert_eq!(backoff_delay(3, None, &c), Duration::from_millis(800));
        assert_eq!(backoff_delay(4, None, &c), Duration::from_millis(1000)); // capped at max
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
            Duration::from_millis(1000) // hint capped at max
        );
    }

    #[test]
    fn retry_succeeds_after_two_429() {
        let c = cfg(3, 10, 100);
        let mut sleeps = Vec::new();
        let mut results = vec![
            Err(GroqError::RateLimit { retry_after: None }),
            Err(GroqError::RateLimit { retry_after: None }),
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
        let out: Result<i32, GroqError> = retry_with(
            &c,
            |d| sleeps.push(d),
            || {
                calls += 1;
                Err(GroqError::BadRequest { detail: None })
            },
        );
        assert!(matches!(out, Err(GroqError::BadRequest { .. })));
        assert_eq!(calls, 1);
        assert!(sleeps.is_empty());
    }

    #[test]
    fn retry_exhausts_on_persistent_5xx() {
        let c = cfg(3, 10, 100);
        let mut sleeps = Vec::new();
        let mut calls = 0;
        let out: Result<i32, GroqError> = retry_with(
            &c,
            |d| sleeps.push(d),
            || {
                calls += 1;
                Err(GroqError::Server(500))
            },
        );
        assert!(matches!(out, Err(GroqError::Server(500))));
        assert_eq!(calls, 4); // 1 initial + 3 retries
        assert_eq!(sleeps.len(), 3);
    }

    #[test]
    fn retry_max_zero_fails_first_attempt() {
        let c = cfg(0, 10, 100);
        let mut sleeps = Vec::new();
        let mut calls = 0;
        let out: Result<i32, GroqError> = retry_with(
            &c,
            |d| sleeps.push(d),
            || {
                calls += 1;
                Err(GroqError::Server(500))
            },
        );
        assert!(matches!(out, Err(GroqError::Server(500))));
        assert_eq!(calls, 1);
        assert!(sleeps.is_empty());
    }
}
