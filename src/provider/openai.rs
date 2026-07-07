//! OpenAI backend (CLO-489). OpenAI-compatible chat-completions with strict
//! `json_schema` Structured Outputs (ADR-001 Decision 7). Default model
//! `gpt-5.4-mini` - non-reasoning, so zero chain-of-thought to suppress.
//! Reasoning-family (`o1`/`o3`/`o4`) `--model` overrides get a dedicated payload
//! path (round-2 review pt 1): no `temperature`, no `system` role,
//! `reasoning_effort` set - else the o-series API 400s.

use serde_json::{json, Value};

use super::http::{self, HttpRequest};
use super::{ErrorKind, Provider, ProviderError};
use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;

const NAME: &str = "OpenAI";
const API_KEY_ENV: &str = "OPENAI_API_KEY";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

pub struct OpenAi {
    model: String,
}

impl OpenAi {
    pub fn new(model: String) -> Self {
        OpenAi { model }
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
        std::env::var("GCM_OPENAI_BASE_URL")
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

impl Provider for OpenAi {
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
        let payload = build_message_payload(&self.model, &super::message_user_content(diff));
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

    fn resolve_hunks(
        &self,
        ctx: &super::ResolveContext,
    ) -> Result<Vec<super::Resolution>, ProviderError> {
        let key = self.api_key()?;
        let payload = build_resolve_payload(ctx, &self.model);
        let raw = http::post_json(&self.request(&key, &payload))?;
        let json = super::extract_openai_content(NAME, &raw)?;
        super::parse_resolutions(NAME, &json, ctx.hunks.len())
    }

    fn cache_model_id(&self) -> String {
        format!("openai:{}", self.model)
    }

    fn diff_budget(&self) -> DiffBudget {
        // Conservative total; gpt-5.4-mini's 400k-token window has ample room.
        DiffBudget::resolve(256_000, DiffBudget::STANDARD_PER_FILE)
    }
}

/// Whether `model` is an OpenAI reasoning family (`o1`/`o3`/`o4`-style): an `o`
/// followed by a digit. Distinguishes `o3-mini` from `gpt-4o-mini` (starts `g`).
fn is_reasoning_model(model: &str) -> bool {
    let mut chars = model.trim().chars();
    matches!(chars.next(), Some('o')) && matches!(chars.next(), Some(c) if c.is_ascii_digit())
}

/// System role for this model: reasoning models reject the `system` role, so use
/// `developer`; non-reasoning models use `system` (round-2 review pt 1).
fn system_role(model: &str) -> &'static str {
    if is_reasoning_model(model) {
        "developer"
    } else {
        "system"
    }
}

/// Add model-family params: `temperature` for non-reasoning models (o-series
/// 400s on a non-default temperature); `reasoning_effort` for reasoning models.
fn apply_model_params(payload: &mut Value, model: &str) {
    let obj = payload.as_object_mut().expect("payload is a JSON object");
    if is_reasoning_model(model) {
        obj.insert("reasoning_effort".into(), json!("low"));
    } else {
        obj.insert("temperature".into(), json!(0.2));
    }
}

/// Add model-family params for resolve payloads, using ctx.temperature.
fn apply_model_params_resolve(payload: &mut Value, model: &str, temperature: f64) {
    let obj = payload.as_object_mut().expect("payload is a JSON object");
    if is_reasoning_model(model) {
        obj.insert("reasoning_effort".into(), json!("low"));
    } else {
        obj.insert("temperature".into(), json!(temperature));
    }
}

fn build_resolve_payload(ctx: &super::ResolveContext, model: &str) -> Value {
    let mut payload = json!({
        "model": model,
        "messages": [
            { "role": system_role(model), "content": super::RESOLVE_SYSTEM_PROMPT },
            { "role": "user", "content": super::resolve_user_content(ctx) },
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "conflict_resolutions",
                "strict": true,
                "schema": super::resolve_schema(),
            }
        }
    });
    apply_model_params_resolve(&mut payload, model, ctx.temperature);
    payload
}

fn build_plan_payload(ctx: &GroupingContext, model: &str) -> Value {
    let mut payload = json!({
        "model": model,
        "messages": [
            { "role": system_role(model), "content": super::GROUPING_SYSTEM_PROMPT },
            { "role": "user", "content": super::grouping_user_content(ctx) },
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
    apply_model_params(&mut payload, model);
    payload
}

fn build_message_payload(model: &str, user_content: &str) -> Value {
    let mut payload = json!({
        "model": model,
        "messages": [
            { "role": system_role(model), "content": super::SYSTEM_PROMPT },
            { "role": "user", "content": user_content },
        ],
    });
    apply_model_params(&mut payload, model);
    payload
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
    fn plan_payload_is_strict_json_schema() {
        let p = build_plan_payload(&ctx(), "gpt-4o-mini-2024-07-18");
        let rf = &p["response_format"];
        assert_eq!(rf["type"], json!("json_schema"));
        assert_eq!(rf["json_schema"]["strict"], json!(true));
        assert!(rf["json_schema"]["schema"]["properties"]["groups"].is_object());
    }

    #[test]
    fn gpt_4o_mini_uses_temperature_and_system_role_no_reasoning() {
        let p = build_plan_payload(&ctx(), "gpt-4o-mini-2024-07-18");
        assert_eq!(p["temperature"], json!(0.2));
        assert!(p.get("reasoning_effort").is_none());
        assert_eq!(p["messages"][0]["role"], json!("system"));
    }

    #[test]
    fn o_series_omits_temperature_uses_developer_role_and_reasoning_effort() {
        // round-2 review pt 1: o-series 400s on temperature + system role.
        for model in ["o1", "o1-mini", "o3-mini", "o4-mini"] {
            let p = build_plan_payload(&ctx(), model);
            assert!(p.get("temperature").is_none(), "{model}: no temperature");
            assert_eq!(
                p["reasoning_effort"],
                json!("low"),
                "{model}: reasoning_effort"
            );
            assert_eq!(
                p["messages"][0]["role"],
                json!("developer"),
                "{model}: developer role"
            );
        }
    }

    #[test]
    fn reasoning_model_detection() {
        assert!(is_reasoning_model("o1"));
        assert!(is_reasoning_model("o3-mini"));
        assert!(is_reasoning_model("o4-mini-2025"));
        assert!(!is_reasoning_model("gpt-4o-mini"));
        assert!(!is_reasoning_model("gpt-4.1"));
        // The default model must take the non-reasoning payload path (system role
        // + temperature); `gpt-5.4-mini` starts with `g`, so it is not detected
        // as an o-series reasoning model.
        assert!(!is_reasoning_model("gpt-5.4-mini"));
        assert!(!is_reasoning_model("openai/gpt-oss-120b"));
    }

    #[test]
    fn cache_model_id_is_provider_qualified() {
        let o = OpenAi::new("gpt-4o-mini-2024-07-18".to_string());
        assert_eq!(o.cache_model_id(), "openai:gpt-4o-mini-2024-07-18");
    }
}
