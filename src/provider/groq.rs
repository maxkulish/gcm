//! Groq backend (CLO-486/487/488, refactored onto the [`Provider`] trait in
//! CLO-489). OpenAI-compatible chat-completions shape with `strict: true`
//! json_schema on gpt-oss; reasoning suppression by model family (capability
//! matrix). The shipped default provider (ADR-001 Decision 5).

use serde_json::{json, Value};

use super::http::{self, HttpRequest};
use super::{ErrorKind, Provider, ProviderError};
use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;

const NAME: &str = "Groq";
const API_KEY_ENV: &str = "GROQ_API_KEY";
const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";

pub struct Groq {
    model: String,
}

impl Groq {
    pub fn new(model: String) -> Self {
        Groq { model }
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
        std::env::var("GCM_GROQ_BASE_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
    }

    fn request<'a>(&self, key: &str, payload: &'a Value) -> HttpRequest<'a> {
        HttpRequest {
            provider: NAME,
            auth_env_var: API_KEY_ENV,
            endpoint: format!("{}/chat/completions", self.base_url().trim_end_matches('/')),
            auth: Some(("Authorization", format!("Bearer {key}"))),
            extra_headers: Vec::new(),
            payload,
        }
    }
}

impl Provider for Groq {
    fn name(&self) -> &'static str {
        NAME
    }

    fn generate_plan(&self, ctx: &GroupingContext) -> Result<Plan, ProviderError> {
        let key = self.api_key()?;
        let payload = build_plan_payload(ctx, &self.model);
        let raw = http::post_json(&self.request(&key, &payload))?;
        let json = super::extract_openai_content(NAME, &raw)?;
        if json.is_empty() {
            return Err(ProviderError {
                provider: NAME,
                kind: ErrorKind::EmptyResponse,
            });
        }
        crate::plan::parse_defensive(&json).map_err(|e| ProviderError {
            provider: NAME,
            kind: ErrorKind::Deserialize(e.to_string()),
        })
    }

    fn generate_message(&self, diff: &GatheredDiff) -> Result<String, ProviderError> {
        let key = self.api_key()?;
        let mut payload = json!({
            "model": self.model,
            "temperature": 0.2,
            "messages": [
                { "role": "system", "content": super::SYSTEM_PROMPT },
                { "role": "user", "content": super::message_user_content(diff) },
            ],
        });
        apply_reasoning_suppression(&mut payload, &self.model);
        let raw = http::post_json(&self.request(&key, &payload))?;
        let message = super::extract_openai_content(NAME, &raw)?;
        if message.is_empty() {
            return Err(ProviderError {
                provider: NAME,
                kind: ErrorKind::EmptyResponse,
            });
        }
        Ok(message)
    }

    fn cache_model_id(&self) -> String {
        format!("groq:{}", self.model)
    }

    fn diff_budget(&self) -> DiffBudget {
        DiffBudget::standard()
    }
}

/// Build the structured-output plan request payload (ADR-001 Decisions 1 & 5).
/// `strict: true` json_schema is **gpt-oss-only** on Groq (capability matrix);
/// other families (e.g. qwen) use best-effort `strict: false`, else Groq 400s.
fn build_plan_payload(ctx: &GroupingContext, model: &str) -> Value {
    let strict = model.contains("gpt-oss");
    let mut payload = json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": super::GROUPING_SYSTEM_PROMPT },
            { "role": "user", "content": super::grouping_user_content(ctx) },
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "commit_plan",
                "strict": strict,
                "schema": crate::plan::schema(),
            }
        }
    });
    apply_reasoning_suppression(&mut payload, model);
    payload
}

/// Select reasoning-suppression params by Groq model family so chain-of-thought
/// never reaches the message (ADR-001 #5; capability matrix). The `<think>`
/// strip is the universal backstop applied to the response regardless.
fn apply_reasoning_suppression(payload: &mut Value, model: &str) {
    let obj = payload.as_object_mut().expect("payload is a JSON object");
    if model.contains("qwen") {
        obj.insert("reasoning_effort".into(), json!("none"));
    } else if model.contains("gpt-oss") {
        obj.insert("include_reasoning".into(), json!(false));
    }
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

    #[test]
    fn plan_payload_requests_strict_structured_output() {
        let p = build_plan_payload(&ctx(), "openai/gpt-oss-120b");
        let rf = &p["response_format"];
        assert_eq!(rf["type"], json!("json_schema"));
        assert_eq!(rf["json_schema"]["name"], json!("commit_plan"));
        assert_eq!(rf["json_schema"]["strict"], json!(true));
        assert!(rf["json_schema"]["schema"]["properties"]["groups"].is_object());
        assert_eq!(p["include_reasoning"], json!(false));
        let user = p["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("Changed files"));
        assert!(user.contains("a.rs"));
        assert!(user.contains("Git status"));
        assert!(user.contains("diff --git"));
    }

    #[test]
    fn qwen_plan_uses_best_effort_strict_false() {
        // Codex validation HIGH: strict json_schema is gpt-oss-only on Groq; a
        // qwen model must request strict:false or Groq 400s.
        let p = build_plan_payload(&ctx(), "qwen/qwen3.6-27b");
        assert_eq!(p["response_format"]["json_schema"]["strict"], json!(false));
        // gpt-oss stays strict:true
        let g = build_plan_payload(&ctx(), "openai/gpt-oss-120b");
        assert_eq!(g["response_format"]["json_schema"]["strict"], json!(true));
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
    fn cache_model_id_is_provider_qualified() {
        let g = Groq::new("openai/gpt-oss-120b".to_string());
        assert_eq!(g.cache_model_id(), "groq:openai/gpt-oss-120b");
    }
}
