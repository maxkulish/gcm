use std::fmt;
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::diff::GatheredDiff;

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

/// Generate a single conventional-commit message for the gathered diff via a
/// direct Groq REST call (FR-10, FR-18). Returns plain text - no JSON plan; the
/// structured grouping contract is out of scope for the tracer.
pub fn generate_commit_message(diff: &GatheredDiff) -> Result<String, GroqError> {
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
    let body =
        serde_json::to_string(&payload).map_err(|e| GroqError::Deserialize(e.to_string()))?;

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

    let raw = response
        .body_mut()
        .read_to_string()
        .map_err(|e| GroqError::Transport(e.to_string()))?;

    let parsed: ChatResponse =
        serde_json::from_str(&raw).map_err(|e| GroqError::Deserialize(e.to_string()))?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();

    let message = strip_think(&content).trim().to_string();
    if message.is_empty() {
        return Err(GroqError::EmptyResponse);
    }
    Ok(message)
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
}
