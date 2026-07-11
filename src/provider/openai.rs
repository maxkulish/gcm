//! OpenAI backend (CLO-489, CLO-545). OpenAI-compatible chat-completions with strict
//! `json_schema` Structured Outputs (ADR-001 Decision 7). gcm supports only the
//! GPT-5.6 family (`gpt-5.6-terra` default, `gpt-5.6-luna`; see [`SUPPORTED_MODELS`]),
//! enforced by the validation gate in `provider::select`. Because the model class is
//! fixed, every request uses one uniform payload with no model-family branching: the
//! `developer` role (GPT-5.6 reasoning models reject `system`), `reasoning_effort:
//! "none"` (omitting it defaults to `medium` - erasing the cost/latency advantage),
//! and a low `temperature`. Reasoning tokens are billed separately and never reach
//! `message.content`, so the shared `strip_think` backstop is defensive only, not the
//! reasoning control.

use serde_json::{json, Value};

use super::http::{self, HttpRequest};
use super::{ErrorKind, Provider, ProviderError};
use crate::diff::{DiffBudget, GatheredDiff, GroupingContext};
use crate::plan::Plan;

const NAME: &str = "OpenAI";
const API_KEY_ENV: &str = "OPENAI_API_KEY";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// The OpenAI models gcm supports (CLO-545). gcm sends a uniform GPT-5.6 payload
/// (`developer` role + `reasoning_effort: "none"` + `temperature`), so only the
/// GPT-5.6 family is valid: `default_model`, the wizard fallback list, and the
/// `provider::select` validation gate all derive from this single source. `[0]` is
/// the default (`terra`, the mini-tier like-for-like per OpenAI's tier mapping).
pub(super) const SUPPORTED_MODELS: [&str; 2] = ["gpt-5.6-terra", "gpt-5.6-luna"];

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
        // Conservative total; GPT-5.6's 1.05M-token window has ample room.
        DiffBudget::resolve(256_000, DiffBudget::STANDARD_PER_FILE)
    }
}

/// Validate that `model` is a supported GPT-5.6 model (CLO-545, Design A). gcm sends a
/// uniform GPT-5.6 payload (`developer` role + `reasoning_effort: "none"`), so a
/// non-5.6 OpenAI model (e.g. a non-reasoning `gpt-4.1`, which rejects
/// `reasoning_effort`) would produce a broken request. Reject it at construction with
/// an actionable error rather than let it 400 downstream. Called from
/// [`super::select`], so it guards both the commit and the resolve flows.
pub(super) fn validate_model(model: &str) -> Result<(), ProviderError> {
    if SUPPORTED_MODELS.contains(&model) {
        Ok(())
    } else {
        Err(ProviderError {
            provider: NAME,
            kind: ErrorKind::Config(format!(
                "OpenAI model '{model}' is not supported; gcm supports {}. \
                 Run `gcm provider` to re-select a model.",
                SUPPORTED_MODELS.join(" or ")
            )),
        })
    }
}

/// The uniform GPT-5.6 message header + reasoning params shared by every builder
/// (CLO-545): the `developer` role (reasoning models reject `system`) and
/// `reasoning_effort: "none"` (omitting it defaults to `medium`). `temperature` is
/// set by each builder (0.2 for plan/message, `ctx.temperature` for resolve).
fn build_resolve_payload(ctx: &super::ResolveContext, model: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            { "role": "developer", "content": super::RESOLVE_SYSTEM_PROMPT },
            { "role": "user", "content": super::resolve_user_content(ctx) },
        ],
        "reasoning_effort": "none",
        "temperature": ctx.temperature,
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "conflict_resolutions",
                "strict": true,
                "schema": super::resolve_schema(),
            }
        }
    })
}

fn build_plan_payload(ctx: &GroupingContext, model: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            { "role": "developer", "content": super::GROUPING_SYSTEM_PROMPT },
            { "role": "user", "content": super::grouping_user_content(ctx) },
        ],
        "reasoning_effort": "none",
        "temperature": 0.2,
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "commit_plan",
                "strict": true,
                "schema": crate::plan::schema(),
            }
        }
    })
}

fn build_message_payload(model: &str, user_content: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            { "role": "developer", "content": super::SYSTEM_PROMPT },
            { "role": "user", "content": user_content },
        ],
        "reasoning_effort": "none",
        "temperature": 0.2,
    })
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

    fn resolve_ctx(temperature: f64) -> super::super::ResolveContext {
        super::super::ResolveContext {
            temperature,
            ..Default::default()
        }
    }

    // The uniform GPT-5.6 payload policy (CLO-545): every builder emits the
    // `developer` role + `reasoning_effort:"none"` (omitting it defaults to `medium`)
    // + `temperature`, with no model-family branching. Exact-shape per builder (BS5).

    #[test]
    fn plan_payload_uniform_gpt_5_6_policy() {
        let p = build_plan_payload(&ctx(), "gpt-5.6-terra");
        assert_eq!(p["model"], json!("gpt-5.6-terra"));
        assert_eq!(p["messages"][0]["role"], json!("developer"));
        assert_eq!(
            p["messages"][0]["content"],
            json!(super::super::GROUPING_SYSTEM_PROMPT)
        );
        assert_eq!(p["reasoning_effort"], json!("none"));
        assert_eq!(p["temperature"], json!(0.2));
        let rf = &p["response_format"];
        assert_eq!(rf["type"], json!("json_schema"));
        assert_eq!(rf["json_schema"]["strict"], json!(true));
        assert!(rf["json_schema"]["schema"]["properties"]["groups"].is_object());
    }

    #[test]
    fn message_payload_uniform_gpt_5_6_policy() {
        let p = build_message_payload("gpt-5.6-terra", "some diff");
        assert_eq!(p["messages"][0]["role"], json!("developer"));
        assert_eq!(
            p["messages"][0]["content"],
            json!(super::super::SYSTEM_PROMPT)
        );
        assert_eq!(p["messages"][1]["role"], json!("user"));
        assert_eq!(p["reasoning_effort"], json!("none"));
        assert_eq!(p["temperature"], json!(0.2));
        assert!(p.get("response_format").is_none());
    }

    #[test]
    fn resolve_payload_uses_ctx_temperature_and_developer_role() {
        let p = build_resolve_payload(&resolve_ctx(0.5), "gpt-5.6-luna");
        assert_eq!(p["model"], json!("gpt-5.6-luna"));
        assert_eq!(p["messages"][0]["role"], json!("developer"));
        assert_eq!(
            p["messages"][0]["content"],
            json!(super::super::RESOLVE_SYSTEM_PROMPT)
        );
        assert_eq!(p["reasoning_effort"], json!("none"));
        assert_eq!(p["temperature"], json!(0.5));
        assert_eq!(p["response_format"]["json_schema"]["strict"], json!(true));
        assert_eq!(
            p["response_format"]["json_schema"]["name"],
            json!("conflict_resolutions")
        );
    }

    #[test]
    fn cache_model_id_is_provider_qualified() {
        let o = OpenAi::new("gpt-5.6-terra".to_string());
        assert_eq!(o.cache_model_id(), "openai:gpt-5.6-terra");
    }

    #[test]
    fn validate_model_accepts_supported_rejects_others() {
        // Design A (CLO-545): only the GPT-5.6 family is valid for OpenAI.
        assert!(validate_model("gpt-5.6-terra").is_ok());
        assert!(validate_model("gpt-5.6-luna").is_ok());
        // A legacy saved model / non-5.6 override is rejected with an actionable
        // Config error naming the model and pointing at `gcm provider`. This is the
        // AC9 breaking-change regression guard; its legacy strings (and the `select`
        // gate test in mod.rs) are the intentional exemptions from the AC5 sweep.
        let err = validate_model("gpt-5.4-mini").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Config(_)));
        let msg = err.to_string();
        assert!(msg.contains("gpt-5.4-mini"), "names the model: {msg}");
        assert!(msg.contains("gcm provider"), "points to recovery: {msg}");
        for bad in ["gpt-4.1", "gpt-4o", "o3-mini", "gpt-5.6-sol"] {
            assert!(validate_model(bad).is_err(), "{bad} must be rejected");
        }
    }
}
