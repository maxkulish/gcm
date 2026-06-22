//! Google Gemini backend (CLO-489). Divergent from the OpenAI-compatible shape:
//! the `:generateContent` endpoint, `x-goog-api-key` auth, structured output via
//! `generationConfig.responseSchema` (OpenAPI-3.0 subset, [`crate::plan::gemini_schema`]),
//! and reasoning suppression via `thinkingConfig.thinkingLevel` (3.x floor is
//! `MINIMAL`; no hard off). The response extractor checks `finishReason` /
//! `promptFeedback` for safety blocks BEFORE reading content (round-2 review pt 3).

use serde::Deserialize;
use serde_json::{json, Value};

use super::http::{self, HttpRequest};
use super::{ErrorKind, Provider, ProviderError};
use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;

const NAME: &str = "Google";
const API_KEY_ENV: &str = "GEMINI_API_KEY";
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

pub struct Gemini {
    model: String,
}

impl Gemini {
    pub fn new(model: String) -> Self {
        Gemini { model }
    }

    fn api_key(&self) -> Result<String, ProviderError> {
        std::env::var(API_KEY_ENV)
            .ok()
            .filter(|k| !k.trim().is_empty())
            .ok_or(ProviderError {
                provider: NAME,
                kind: ErrorKind::MissingKey {
                    env_var: API_KEY_ENV,
                },
            })
    }

    /// Base URL: `GCM_GEMINI_BASE_URL` (primary) or `GCM_GOOGLE_BASE_URL` (alias),
    /// else the default (round-2 review pt 4).
    fn base_url(&self) -> String {
        std::env::var("GCM_GEMINI_BASE_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .or_else(|| {
                std::env::var("GCM_GOOGLE_BASE_URL")
                    .ok()
                    .filter(|u| !u.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
    }

    fn request<'a>(&self, key: &str, payload: &'a Value) -> HttpRequest<'a> {
        HttpRequest {
            provider: NAME,
            auth_env_var: API_KEY_ENV,
            endpoint: format!(
                "{}/v1beta/models/{}:generateContent",
                self.base_url().trim_end_matches('/'),
                self.model
            ),
            auth: ("x-goog-api-key", key.to_string()),
            extra_headers: Vec::new(),
            payload,
        }
    }
}

impl Provider for Gemini {
    fn name(&self) -> &'static str {
        NAME
    }

    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
        let key = self.api_key()?;
        let payload = build_plan_payload(ctx);
        let raw = http::post_json(&self.request(&key, &payload))?;
        let json = extract_text(&raw)?;
        if json.is_empty() {
            return Err(empty());
        }
        crate::plan::parse_defensive(&json).map_err(|e| ProviderError {
            provider: NAME,
            kind: ErrorKind::Deserialize(e.to_string()),
        })
    }

    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
        let key = self.api_key()?;
        let payload = build_message_payload(&super::message_user_content(diff));
        let raw = http::post_json(&self.request(&key, &payload))?;
        let message = extract_text(&raw)?;
        if message.is_empty() {
            return Err(empty());
        }
        Ok(message)
    }

    fn cache_model_id(&self) -> String {
        format!("google:{}", self.model)
    }

    fn diff_budget(&self) -> DiffBudget {
        // gemini-3.1-flash-lite has a very large context; standard budget for v1.
        DiffBudget::standard()
    }
}

fn empty() -> ProviderError {
    ProviderError {
        provider: NAME,
        kind: ErrorKind::EmptyResponse,
    }
}

fn build_plan_payload(ctx: &GroupingContext) -> Value {
    json!({
        "systemInstruction": { "parts": [ { "text": super::GROUPING_SYSTEM_PROMPT } ] },
        "contents": [ { "role": "user", "parts": [ { "text": super::grouping_user_content(ctx) } ] } ],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": crate::plan::gemini_schema(),
            "thinkingConfig": { "thinkingLevel": "MINIMAL" }
        }
    })
}

fn build_message_payload(user_content: &str) -> Value {
    json!({
        "systemInstruction": { "parts": [ { "text": super::SYSTEM_PROMPT } ] },
        "contents": [ { "role": "user", "parts": [ { "text": user_content } ] } ],
        "generationConfig": {
            "thinkingConfig": { "thinkingLevel": "MINIMAL" }
        }
    })
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
    #[serde(rename = "promptFeedback")]
    prompt_feedback: Option<PromptFeedback>,
}

#[derive(Deserialize)]
struct PromptFeedback {
    #[serde(rename = "blockReason")]
    block_reason: Option<String>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<Content>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Content {
    parts: Option<Vec<Part>>,
}

#[derive(Deserialize)]
struct Part {
    text: Option<String>,
    thought: Option<bool>,
}

/// Extract the answer text from a Gemini response. Checks for prompt- and
/// candidate-level blocks BEFORE reading content (a safety block returns 200 OK
/// with no content), concatenates non-thought parts, and applies the `<think>`
/// backstop. Returns `Ok("")` for a genuinely empty STOP response (the caller
/// maps that to `EmptyResponse`); blocks/truncation are typed errors.
fn extract_text(raw: &str) -> Result<String, ProviderError> {
    let err = |kind| ProviderError {
        provider: NAME,
        kind,
    };
    let resp: GeminiResponse =
        serde_json::from_str(raw).map_err(|e| err(ErrorKind::Deserialize(e.to_string())))?;

    if let Some(reason) = resp
        .prompt_feedback
        .as_ref()
        .and_then(|p| p.block_reason.as_deref())
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        return Err(err(ErrorKind::BadRequest {
            detail: Some(format!("Gemini blocked the prompt (reason: {reason})")),
        }));
    }

    let Some(cand) = resp.candidates.as_ref().and_then(|c| c.first()) else {
        return Err(err(ErrorKind::EmptyResponse));
    };

    if let Some(fr) = cand.finish_reason.as_deref() {
        match fr {
            "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
                return Err(err(ErrorKind::BadRequest {
                    detail: Some(format!("Gemini blocked the response (finishReason: {fr})")),
                }));
            }
            "MAX_TOKENS" => {
                return Err(err(ErrorKind::Deserialize(
                    "Gemini response truncated (finishReason: MAX_TOKENS); the diff may be too large"
                        .to_string(),
                )));
            }
            _ => {}
        }
    }

    let text: String = cand
        .content
        .as_ref()
        .and_then(|c| c.parts.as_ref())
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p.thought != Some(true))
                .filter_map(|p| p.text.as_deref())
                .collect::<String>()
        })
        .unwrap_or_default();

    Ok(super::strip_think(&text).trim().to_string())
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
    fn plan_payload_has_response_schema_and_thinking() {
        let p = build_plan_payload(&ctx());
        let gc = &p["generationConfig"];
        assert_eq!(gc["responseMimeType"], json!("application/json"));
        assert_eq!(gc["thinkingConfig"]["thinkingLevel"], json!("MINIMAL"));
        // OpenAPI-subset schema (upper-case types)
        assert_eq!(gc["responseSchema"]["type"], json!("OBJECT"));
        assert_eq!(
            p["systemInstruction"]["parts"][0]["text"],
            json!(super::super::GROUPING_SYSTEM_PROMPT)
        );
        assert!(p["contents"][0]["parts"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Changed files"));
    }

    #[test]
    fn message_payload_has_no_response_schema() {
        let p = build_message_payload("Diff stats:\nx\n\nFull diff:\ny");
        assert!(p["generationConfig"].get("responseSchema").is_none());
        assert_eq!(
            p["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            json!("MINIMAL")
        );
    }

    #[test]
    fn extract_concatenates_non_thought_parts() {
        let raw = r#"{"candidates":[{"content":{"parts":[
            {"text":"thinking...","thought":true},
            {"text":"{\"groups\":[]}"}
        ]},"finishReason":"STOP"}]}"#;
        assert_eq!(extract_text(raw).unwrap(), r#"{"groups":[]}"#);
    }

    #[test]
    fn extract_safety_block_is_typed_error_not_empty() {
        // round-2 review pt 3: 200 OK, no content, finishReason SAFETY.
        let raw = r#"{"candidates":[{"finishReason":"SAFETY"}]}"#;
        let e = extract_text(raw).unwrap_err();
        match e.kind {
            ErrorKind::BadRequest { detail: Some(d) } => {
                assert!(d.contains("SAFETY"), "names the reason");
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn extract_prompt_block_is_typed_error() {
        let raw = r#"{"promptFeedback":{"blockReason":"BLOCKLIST"}}"#;
        let e = extract_text(raw).unwrap_err();
        assert!(matches!(e.kind, ErrorKind::BadRequest { .. }));
        assert!(e.to_string().contains("BLOCKLIST"));
    }

    #[test]
    fn extract_thought_only_is_empty() {
        // All parts are thought parts -> empty answer (caller maps to EmptyResponse).
        let raw = r#"{"candidates":[{"content":{"parts":[{"text":"deliberating","thought":true}]},"finishReason":"STOP"}]}"#;
        assert_eq!(extract_text(raw).unwrap(), "");
    }

    #[test]
    fn extract_max_tokens_is_typed_error() {
        let raw =
            r#"{"candidates":[{"content":{"parts":[{"text":"{"}]},"finishReason":"MAX_TOKENS"}]}"#;
        assert!(matches!(
            extract_text(raw).unwrap_err().kind,
            ErrorKind::Deserialize(_)
        ));
    }

    #[test]
    fn extract_no_candidates_is_empty_response() {
        assert!(matches!(
            extract_text(r#"{"candidates":[]}"#).unwrap_err().kind,
            ErrorKind::EmptyResponse
        ));
    }

    #[test]
    fn cache_model_id_is_provider_qualified() {
        let g = Gemini::new("gemini-3.1-flash-lite".to_string());
        assert_eq!(g.cache_model_id(), "google:gemini-3.1-flash-lite");
    }
}
