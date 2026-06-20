use std::fmt;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::diff::{GatheredDiff, GroupingContext};
use crate::plan::Plan;

const DEFAULT_MODEL: &str = "openai/gpt-oss-120b";
const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
const TIMEOUT_SECS: u64 = 30;

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

/// Errors from the Groq message call. A light taxonomy for the tracer; the full
/// typed-error/retry surface (FR-21/22) lands in CLO-488.
#[derive(Debug)]
pub enum GroqError {
    MissingKey,
    Http(u16),
    Timeout,
    Transport(String),
    EmptyResponse,
    Deserialize(String),
}

impl fmt::Display for GroqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroqError::MissingKey => write!(
                f,
                "GROQ_API_KEY is not set. Export it (e.g. `export GROQ_API_KEY=...`) and retry."
            ),
            GroqError::Http(code) => write!(f, "Groq API returned HTTP {code}"),
            GroqError::Timeout => write!(f, "Groq API request timed out after {TIMEOUT_SECS}s"),
            GroqError::Transport(msg) => write!(f, "could not reach the Groq API: {msg}"),
            GroqError::EmptyResponse => write!(f, "Groq returned an empty commit message"),
            GroqError::Deserialize(msg) => write!(f, "could not parse the Groq response: {msg}"),
        }
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

/// The configured model id (`GCM_GROQ_MODEL` or the default), resolved
/// **without** requiring `GROQ_API_KEY`. Used by the plan cache to fold the
/// model into the freshness fingerprint (CLO-491, FR-27) even when no key is set.
pub fn resolved_model() -> String {
    std::env::var("GCM_GROQ_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

/// Resolve `(api_key, model, base_url)` from the environment - shared by the
/// message (tracer) and plan (grouping) calls.
fn resolve_config() -> Result<(String, String, String), GroqError> {
    let key = std::env::var("GROQ_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or(GroqError::MissingKey)?;
    let model = resolved_model();
    let base_url = std::env::var("GCM_GROQ_BASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    Ok((key, model, base_url))
}

/// POST a chat-completions payload and return the raw response body. Shared
/// transport (30s timeout, HTTP-status-as-error) for both calls.
fn send_chat(key: &str, base_url: &str, payload: &Value) -> Result<String, GroqError> {
    let body = serde_json::to_string(payload).map_err(|e| GroqError::Deserialize(e.to_string()))?;
    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_SECS)))
        .http_status_as_error(true)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent
        .post(&endpoint)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send(body.as_str())
        .map_err(map_ureq_error)?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| GroqError::Transport(e.to_string()))
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
    serde_json::from_str(&json).map_err(|e| GroqError::Deserialize(e.to_string()))
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
}
