//! Anthropic backend (CLO-494): forced tool-use for structured output.
//!
//! Anthropic has no generic `response_format` like the OpenAI-compatible
//! providers. Structured output is obtained via forced tool-use: define a tool
//! whose `input_schema` is the Plan schema, force the call with
//! `tool_choice: {type: "tool"}`, and extract the typed plan from the
//! `tool_use` content block.
//!
//! Reasoning suppression: Anthropic's default adaptive thinking omits visible
//! CoT by default. The universal `strip_think()` backstop handles any leakage.
//!
//! Auth: `x-api-key` header (not Bearer). Required extra header:
//! `anthropic-version: 2023-06-01`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::http::{self, HttpRequest};
use super::{ErrorKind, Provider, ProviderError};
use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;

const NAME: &str = "Anthropic";
const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct Anthropic {
    model: String,
}

impl Anthropic {
    pub fn new(model: String) -> Self {
        Anthropic { model }
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

    fn base_url(&self) -> String {
        std::env::var("GCM_ANTHROPIC_BASE_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
    }

    fn request<'a>(&self, key: &str, payload: &'a Value) -> HttpRequest<'a> {
        HttpRequest {
            provider: NAME,
            auth_env_var: API_KEY_ENV,
            endpoint: format!("{}/v1/messages", self.base_url().trim_end_matches('/')),
            auth: Some(("x-api-key", key.to_string())),
            extra_headers: vec![("anthropic-version", ANTHROPIC_VERSION.to_string())],
            payload,
        }
    }
}

impl Provider for Anthropic {
    fn name(&self) -> &'static str {
        NAME
    }

    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
        let key = self.api_key()?;
        let payload = build_plan_payload(ctx, &self.model);
        let raw = http::post_json(&self.request(&key, &payload))?;
        let json_str = extract_tool_use_input(NAME, &raw)?;
        if json_str.is_empty() {
            return Err(ProviderError {
                provider: NAME,
                kind: ErrorKind::EmptyResponse,
            });
        }
        // Attempt direct deserialization first (review suggestion 3): the
        // tool_use input is already a parsed JSON value, so try from_value
        // before falling back to parse_defensive.
        if let Ok(plan) = serde_json::from_str::<Plan>(&json_str) {
            return Ok(plan);
        }
        crate::plan::parse_defensive(&json_str).map_err(|e| ProviderError {
            provider: NAME,
            kind: ErrorKind::Deserialize(e.to_string()),
        })
    }

    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
        let key = self.api_key()?;
        let payload = build_message_payload(&super::message_user_content(diff), &self.model);
        let raw = http::post_json(&self.request(&key, &payload))?;
        let message = extract_text_content(NAME, &raw)?;
        if message.is_empty() {
            return Err(ProviderError {
                provider: NAME,
                kind: ErrorKind::EmptyResponse,
            });
        }
        Ok(message)
    }

    fn resolve_hunks(
        &self,
        ctx: &super::ResolveContext,
    ) -> Result<Vec<super::Resolution>, ProviderError> {
        let key = self.api_key()?;
        let payload = build_resolve_payload(ctx, &self.model);
        let raw = http::post_json(&self.request(&key, &payload))?;
        let json_str = extract_tool_use_input(NAME, &raw)?;
        if json_str.is_empty() {
            return Err(ProviderError {
                provider: NAME,
                kind: ErrorKind::EmptyResponse,
            });
        }
        super::parse_resolutions(NAME, &json_str, ctx.hunks.len())
    }

    fn cache_model_id(&self) -> String {
        format!("anthropic:{}", self.model)
    }

    fn diff_budget(&self) -> DiffBudget {
        DiffBudget::standard()
    }
}

// ---------------------------------------------------------------------------
// Payload builders
// ---------------------------------------------------------------------------

fn build_resolve_payload(ctx: &super::ResolveContext, model: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": 4096,
        "system": super::RESOLVE_SYSTEM_PROMPT,
        "messages": [
            { "role": "user", "content": super::resolve_user_content(ctx) }
        ],
        "tools": [{
            "name": "conflict_resolutions",
            "description": "Return the resolved conflict hunks",
            "input_schema": super::resolve_schema()
        }],
        "tool_choice": { "type": "tool", "name": "conflict_resolutions" }
    })
}

/// Build the forced tool-use plan request payload.
///
/// Uses `plan::schema()` as the tool's `input_schema` and forces the call with
/// `tool_choice: {type: "tool", name: "commit_plan"}`.
fn build_plan_payload(ctx: &GroupingContext, model: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": 4096,
        "system": super::GROUPING_SYSTEM_PROMPT,
        "messages": [
            { "role": "user", "content": super::grouping_user_content(ctx) }
        ],
        "tools": [{
            "name": "commit_plan",
            "description": "Return the commit grouping plan",
            "input_schema": crate::plan::schema()
        }],
        "tool_choice": { "type": "tool", "name": "commit_plan" }
    })
}

/// Build the plain-text message request payload (no tools, no tool_choice).
fn build_message_payload(user_content: &str, model: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": 1024,
        "system": super::SYSTEM_PROMPT,
        "messages": [
            { "role": "user", "content": user_content }
        ]
    })
}

// ---------------------------------------------------------------------------
// Response parsers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[allow(dead_code)]
        id: String,
        #[allow(dead_code)]
        name: String,
        input: Value,
    },
    #[serde(rename = "thinking")]
    Thinking {
        #[allow(dead_code)]
        thinking: String,
    },
}

/// Extract the `tool_use` input from an Anthropic Messages API response.
///
/// Returns the `input` JSON object serialized back to a string (for
/// `parse_defensive`), or a typed error for edge cases:
///
/// - `stop_reason: "max_tokens"` → `Deserialize` error with truncation message
/// - `stop_reason: "end_turn"` with text blocks → extract text for fallback
/// - `stop_reason: "refusal"` → `BadRequest` error
/// - Empty content → `EmptyResponse`
/// - `thinking` blocks are silently skipped
fn extract_tool_use_input(provider: &'static str, raw: &str) -> Result<String, ProviderError> {
    let err = |kind| ProviderError { provider, kind };
    let resp: AnthropicResponse =
        serde_json::from_str(raw).map_err(|e| err(ErrorKind::Deserialize(e.to_string())))?;

    let stop_reason = resp.stop_reason.as_deref().unwrap_or("");

    // Check for max_tokens truncation first (matches Gemini's MAX_TOKENS handling).
    if stop_reason == "max_tokens" {
        return Err(err(ErrorKind::Deserialize(
            "Anthropic response truncated (stop_reason: max_tokens); the diff may be too large"
                .to_string(),
        )));
    }

    // Find the first tool_use block.
    for block in &resp.content {
        if let ContentBlock::ToolUse { input, .. } = block {
            // Attempt direct deserialization first (review suggestion 3).
            if let Ok(plan) = serde_json::from_value::<Plan>(input.clone()) {
                return serde_json::to_string(&plan)
                    .map_err(|e| err(ErrorKind::Deserialize(e.to_string())));
            }
            // Fall back: serialize input to string for parse_defensive.
            return serde_json::to_string(input)
                .map_err(|e| err(ErrorKind::Deserialize(e.to_string())));
        }
    }

    // No tool_use block found — check stop_reason for fallback paths.
    match stop_reason {
        "end_turn" => {
            // Extract text blocks for parse_defensive fallback.
            let text: String = resp
                .content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if text.is_empty() {
                return Err(err(ErrorKind::EmptyResponse));
            }
            Ok(super::strip_think(&text).trim().to_string())
        }
        "refusal" => {
            let detail = resp
                .content
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .next();
            Err(err(ErrorKind::BadRequest { detail }))
        }
        _ => Err(err(ErrorKind::EmptyResponse)),
    }
}

/// Extract text content from an Anthropic Messages API response (for
/// `generate_message`). Concatenates all `text` content blocks, applies
/// `strip_think()`, and trims. Returns empty string if no text blocks.
fn extract_text_content(provider: &'static str, raw: &str) -> Result<String, ProviderError> {
    let err = |kind| ProviderError { provider, kind };
    let resp: AnthropicResponse =
        serde_json::from_str(raw).map_err(|e| err(ErrorKind::Deserialize(e.to_string())))?;

    let text: String = resp
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text { text } = b {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect();

    Ok(super::strip_think(&text).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> GroupingContext {
        GroupingContext {
            file_list: "a.rs\nb.md".to_string(),
            status: " M a.rs\n?? b.md".to_string(),
            stat: "2 files changed".to_string(),
            body: "diff --git a/a.rs b/a.rs".to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Payload builder tests
    // -----------------------------------------------------------------------

    #[test]
    fn build_plan_payload_shape() {
        let p = build_plan_payload(&ctx(), "claude-haiku-4-5");
        assert_eq!(p["model"], json!("claude-haiku-4-5"));
        assert_eq!(p["max_tokens"], json!(4096));
        assert_eq!(p["system"], json!(super::super::GROUPING_SYSTEM_PROMPT));
        assert_eq!(p["messages"][0]["role"], json!("user"));
        assert!(p["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Changed files"));

        // Tools array with commit_plan tool
        let tools = p["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], json!("commit_plan"));
        assert_eq!(
            tools[0]["description"],
            json!("Return the commit grouping plan")
        );
        assert!(tools[0]["input_schema"]["properties"]["groups"].is_object());

        // tool_choice forces the tool
        assert_eq!(p["tool_choice"]["type"], json!("tool"));
        assert_eq!(p["tool_choice"]["name"], json!("commit_plan"));
    }

    #[test]
    fn build_message_payload_shape() {
        let p = build_message_payload("Diff stats:\nx\n\nFull diff:\ny", "claude-haiku-4-5");
        assert_eq!(p["model"], json!("claude-haiku-4-5"));
        assert_eq!(p["max_tokens"], json!(1024));
        assert_eq!(p["system"], json!(super::super::SYSTEM_PROMPT));
        assert_eq!(p["messages"][0]["role"], json!("user"));
        assert!(p.get("tools").is_none());
        assert!(p.get("tool_choice").is_none());
    }

    // -----------------------------------------------------------------------
    // extract_tool_use_input tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_tool_use_input_happy_path() {
        let raw = r#"{
            "content": [
                { "type": "tool_use", "id": "toolu_abc", "name": "commit_plan",
                  "input": { "groups": [{"files": ["a.rs"], "summary": "core", "commit_message": "feat: add a"}] } }
            ],
            "stop_reason": "tool_use"
        }"#;
        let result = extract_tool_use_input(NAME, raw).unwrap();
        // Should be valid JSON containing the groups
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["groups"].is_array());
        assert_eq!(parsed["groups"][0]["files"][0], json!("a.rs"));
    }

    #[test]
    fn extract_tool_use_input_end_turn_fallback() {
        let raw = r#"{
            "content": [
                { "type": "text", "text": "{\"groups\":[{\"files\":[\"a.rs\"],\"summary\":\"core\",\"commit_message\":\"feat: a\"}]}" }
            ],
            "stop_reason": "end_turn"
        }"#;
        let result = extract_tool_use_input(NAME, raw).unwrap();
        assert!(result.contains("groups"));
        assert!(result.contains("a.rs"));
    }

    #[test]
    fn extract_tool_use_input_refusal() {
        let raw = r#"{
            "content": [
                { "type": "text", "text": "I cannot process this request." }
            ],
            "stop_reason": "refusal"
        }"#;
        let err = extract_tool_use_input(NAME, raw).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::BadRequest { .. }));
    }

    #[test]
    fn extract_tool_use_input_max_tokens() {
        let raw = r#"{
            "content": [
                { "type": "tool_use", "id": "toolu_abc", "name": "commit_plan",
                  "input": { "groups": [] } }
            ],
            "stop_reason": "max_tokens"
        }"#;
        let err = extract_tool_use_input(NAME, raw).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Deserialize(_)));
        assert!(err.to_string().contains("max_tokens"));
    }

    #[test]
    fn extract_tool_use_input_empty_content() {
        let raw = r#"{
            "content": [],
            "stop_reason": "end_turn"
        }"#;
        let err = extract_tool_use_input(NAME, raw).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::EmptyResponse));
    }

    #[test]
    fn extract_tool_use_input_skips_thinking_blocks() {
        let raw = r#"{
            "content": [
                { "type": "thinking", "thinking": "I need to group these files..." },
                { "type": "tool_use", "id": "toolu_abc", "name": "commit_plan",
                  "input": { "groups": [{"files": ["a.rs"], "summary": "core", "commit_message": "feat: a"}] } }
            ],
            "stop_reason": "tool_use"
        }"#;
        let result = extract_tool_use_input(NAME, raw).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["groups"].is_array());
    }

    #[test]
    fn extract_tool_use_input_direct_deserialization() {
        // When input is a valid Plan JSON value, from_value<Plan> succeeds
        // directly without falling back to parse_defensive.
        let raw = r#"{
            "content": [
                { "type": "tool_use", "id": "toolu_abc", "name": "commit_plan",
                  "input": { "groups": [{"files": ["a.rs"], "summary": "core", "commit_message": "feat: a"}] } }
            ],
            "stop_reason": "tool_use"
        }"#;
        let result = extract_tool_use_input(NAME, raw).unwrap();
        // The result should be a valid Plan JSON string
        let plan: Plan = serde_json::from_str(&result).unwrap();
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].files, vec!["a.rs".to_string()]);
        assert_eq!(plan.groups[0].summary, "core");
    }

    // -----------------------------------------------------------------------
    // extract_text_content tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_text_content_happy_path() {
        let raw = r#"{
            "content": [
                { "type": "text", "text": "feat: add thing" }
            ],
            "stop_reason": "end_turn"
        }"#;
        assert_eq!(extract_text_content(NAME, raw).unwrap(), "feat: add thing");
    }

    #[test]
    fn extract_text_content_skips_thinking() {
        let raw = r#"{
            "content": [
                { "type": "thinking", "thinking": "I should write a commit message..." },
                { "type": "text", "text": "feat: add thing" }
            ],
            "stop_reason": "end_turn"
        }"#;
        assert_eq!(extract_text_content(NAME, raw).unwrap(), "feat: add thing");
    }

    #[test]
    fn extract_text_content_concatenates_multiple_text_blocks() {
        let raw = r#"{
            "content": [
                { "type": "text", "text": "feat: add thing" },
                { "type": "text", "text": "\n\nMore details." }
            ],
            "stop_reason": "end_turn"
        }"#;
        assert_eq!(
            extract_text_content(NAME, raw).unwrap(),
            "feat: add thing\n\nMore details."
        );
    }

    // -----------------------------------------------------------------------
    // Helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn cache_model_id_is_provider_qualified() {
        let a = Anthropic::new("claude-haiku-4-5".to_string());
        assert_eq!(a.cache_model_id(), "anthropic:claude-haiku-4-5");
    }

    #[test]
    fn base_url_resolution_default_and_override() {
        // Both cases mutate the same process-global env var, so they share one
        // test body; splitting them into separate #[test] fns races under
        // cargo's parallel execution.
        let prev = std::env::var("GCM_ANTHROPIC_BASE_URL").ok();

        std::env::remove_var("GCM_ANTHROPIC_BASE_URL");
        let a = Anthropic::new("claude-haiku-4-5".to_string());
        assert_eq!(a.base_url(), "https://api.anthropic.com");

        std::env::set_var("GCM_ANTHROPIC_BASE_URL", "http://localhost:8080");
        let a = Anthropic::new("claude-haiku-4-5".to_string());
        assert_eq!(a.base_url(), "http://localhost:8080");

        // Restore
        if let Some(u) = prev {
            std::env::set_var("GCM_ANTHROPIC_BASE_URL", u);
        } else {
            std::env::remove_var("GCM_ANTHROPIC_BASE_URL");
        }
    }

    #[test]
    fn request_sends_correct_headers() {
        // Verify the request() method produces the expected auth + extra headers.
        // Guard against a leftover env override from the parallel base-url test.
        let prev = std::env::var("GCM_ANTHROPIC_BASE_URL").ok();
        std::env::remove_var("GCM_ANTHROPIC_BASE_URL");
        let a = Anthropic::new("claude-haiku-4-5".to_string());
        let payload = serde_json::json!({"model": "test"});
        let req = a.request("sk-ant-test", &payload);
        if let Some(u) = prev {
            std::env::set_var("GCM_ANTHROPIC_BASE_URL", u);
        }
        let (auth_name, auth_value) = req.auth.as_ref().expect("anthropic sends an auth header");
        assert_eq!(*auth_name, "x-api-key");
        assert_eq!(auth_value, "sk-ant-test");
        assert!(req
            .extra_headers
            .iter()
            .any(|(n, v)| { *n == "anthropic-version" && v == "2023-06-01" }));
        assert_eq!(req.endpoint, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn api_key_missing_is_typed_error() {
        // Temporarily unset ANTHROPIC_API_KEY
        let prev = std::env::var(API_KEY_ENV).ok();
        std::env::remove_var(API_KEY_ENV);
        let a = Anthropic::new("claude-haiku-4-5".to_string());
        let err = a.api_key().unwrap_err();
        assert!(matches!(
            err.kind,
            ErrorKind::MissingKey {
                env_var: "ANTHROPIC_API_KEY"
            }
        ));
        // Restore
        if let Some(k) = prev {
            std::env::set_var(API_KEY_ENV, k);
        }
    }
}
