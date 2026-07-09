//! Model-list discovery for the interactive `gcm provider` wizard (CLO-516).
//!
//! Best-effort: each provider's live model endpoint is queried (short timeout, one
//! light retry via [`super::http::get_json`]); on *any* failure - no key, transport
//! error, non-2xx, unparseable body, or an empty result - it degrades to a static
//! per-provider fallback list so the wizard spinner always resolves to a usable set.
//! The raw list is post-processed (D7): non-chat models filtered out, static
//! baselines (including the provider's `default_model`) merged in, then deduped.
//!
//! Centralized here (rather than spread across the five backends) deliberately:
//! discovery is fallback-safe, so a base-URL drift only costs a fallback, not a
//! runtime error. The hot commit path stays in the backends, untouched.

use serde_json::Value;

use super::http::{self, HttpGet};
use super::ProviderId;

/// Where a model list came from, so the wizard can message accurately.
pub enum FetchSource {
    Live,
    Fallback,
}

/// The outcome of a model-list fetch: the (filtered, merged, deduped) models, the
/// source, and an optional non-fatal warning to surface in the wizard.
pub struct ModelFetchOutcome {
    pub models: Vec<String>,
    pub source: FetchSource,
    pub warning: Option<String>,
}

/// Fetch the provider's available models for the wizard. Never errors: returns a
/// usable list in every case (live + baselines, or the static fallback).
/// `key` is the resolved API key (None for Ollama, or a cloud provider with none
/// yet); `endpoint` is an explicit base URL (Ollama, from the wizard).
pub fn fetch_supported_models(
    id: ProviderId,
    key: Option<&str>,
    endpoint: Option<&str>,
) -> ModelFetchOutcome {
    let key = key.map(str::trim).filter(|k| !k.is_empty());

    // Vertex (CLO-537): keyless ADC, no live models endpoint in the MVP (design D4),
    // so return the static Gemini set directly. This short-circuit also keeps the
    // exhaustive `match id` arms below unreachable for Vertex at runtime.
    if id == ProviderId::Vertex {
        return ModelFetchOutcome {
            models: static_fallback_models(id),
            source: FetchSource::Fallback,
            warning: None,
        };
    }

    // No-key short-circuit (D7.2): a key-bearing provider with no key can't fetch,
    // so skip the network call and show the built-in list with an explicit note.
    if let Some(var) = id.key_env_var() {
        if key.is_none() {
            return ModelFetchOutcome {
                models: static_fallback_models(id),
                source: FetchSource::Fallback,
                warning: Some(format!(
                    "no {var} set - showing the built-in model list; provide the key for the live catalog"
                )),
            };
        }
    }

    match fetch_live(id, key, endpoint) {
        Ok(raw) => {
            let live: Vec<String> = raw.into_iter().filter(|m| keep_chat_model(id, m)).collect();
            let live_count = live.len();
            // Merge static baselines (incl. default_model) so known-good models are
            // always selectable even if the live list omits them (D7.3).
            let mut models = live;
            models.extend(static_fallback_models(id));
            let models = dedupe(models);
            if live_count == 0 {
                ModelFetchOutcome {
                    models,
                    source: FetchSource::Fallback,
                    warning: Some(format!(
                        "{} returned no usable models; using the built-in list",
                        id.as_str()
                    )),
                }
            } else {
                ModelFetchOutcome {
                    models,
                    source: FetchSource::Live,
                    warning: None,
                }
            }
        }
        Err(e) => ModelFetchOutcome {
            models: static_fallback_models(id),
            source: FetchSource::Fallback,
            warning: Some(format!(
                "could not fetch {} models ({e}); using the built-in list",
                id.as_str()
            )),
        },
    }
}

/// Query the live model-list endpoint and parse it into raw ids (unfiltered).
fn fetch_live(
    id: ProviderId,
    key: Option<&str>,
    endpoint: Option<&str>,
) -> Result<Vec<String>, super::ProviderError> {
    let base = resolved_base_url(id, endpoint);
    let base = base.trim_end_matches('/');
    let name = provider_name(id);
    let env_var = id.key_env_var().unwrap_or("");
    let req = match id {
        ProviderId::Groq | ProviderId::Openai => HttpGet {
            provider: name,
            auth_env_var: env_var,
            endpoint: format!("{base}/models"),
            auth: key.map(|k| ("Authorization", format!("Bearer {k}"))),
            extra_headers: Vec::new(),
        },
        ProviderId::Anthropic => HttpGet {
            provider: name,
            auth_env_var: env_var,
            endpoint: format!("{base}/v1/models?limit=1000"),
            auth: key.map(|k| ("x-api-key", k.to_string())),
            extra_headers: vec![("anthropic-version", "2023-06-01".to_string())],
        },
        // Vertex is short-circuited in fetch_supported_models; this arm only
        // satisfies exhaustiveness and never runs.
        ProviderId::Google | ProviderId::Vertex => HttpGet {
            provider: name,
            auth_env_var: env_var,
            endpoint: format!("{base}/v1beta/models?pageSize=1000"),
            auth: key.map(|k| ("x-goog-api-key", k.to_string())),
            extra_headers: Vec::new(),
        },
        ProviderId::Ollama => HttpGet {
            provider: name,
            auth_env_var: env_var,
            endpoint: format!("{base}/api/tags"),
            auth: None,
            extra_headers: Vec::new(),
        },
    };
    let raw = http::get_json(&req)?;
    Ok(parse_models(id, &raw))
}

/// Resolve the model-list base URL: an explicit `endpoint` (Ollama, from the
/// wizard) wins, else the provider's `GCM_*_BASE_URL` override, else its default.
/// Mirrors the backends' base URLs (the runtime source of truth); a drift only
/// costs a fallback since fetch is best-effort.
fn resolved_base_url(id: ProviderId, endpoint: Option<&str>) -> String {
    resolved_base_url_with(id, endpoint, |v| std::env::var(v).ok())
}

/// Body of [`resolved_base_url`] with the env lookup injected (hermetic tests).
/// Env var precedence per provider mirrors the backends. Google reads both
/// `GCM_GEMINI_BASE_URL` (primary) and the `GCM_GOOGLE_BASE_URL` alias, matching
/// `gemini.rs` - otherwise an alias-based setup fetches from the wrong endpoint.
fn resolved_base_url_with(
    id: ProviderId,
    endpoint: Option<&str>,
    lookup: impl Fn(&str) -> Option<String>,
) -> String {
    if let Some(e) = endpoint.map(str::trim).filter(|e| !e.is_empty()) {
        return e.to_string();
    }
    let (env_vars, default): (&[&str], &str) = match id {
        ProviderId::Groq => (&["GCM_GROQ_BASE_URL"], "https://api.groq.com/openai/v1"),
        ProviderId::Openai => (&["GCM_OPENAI_BASE_URL"], "https://api.openai.com/v1"),
        ProviderId::Anthropic => (&["GCM_ANTHROPIC_BASE_URL"], "https://api.anthropic.com"),
        ProviderId::Google | ProviderId::Vertex => (
            &["GCM_GEMINI_BASE_URL", "GCM_GOOGLE_BASE_URL"],
            "https://generativelanguage.googleapis.com",
        ),
        ProviderId::Ollama => (&["GCM_OLLAMA_BASE_URL"], "http://localhost:11434"),
    };
    env_vars
        .iter()
        .find_map(|var| {
            lookup(var)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .unwrap_or_else(|| default.to_string())
}

/// Parse a provider's model-list body into raw ids (tolerant of extra fields;
/// returns empty on any shape mismatch). Gemini is filtered to `generateContent`
/// models here (the authoritative capability signal) and de-prefixed.
fn parse_models(id: ProviderId, body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    match id {
        // OpenAI-compatible: { "data": [ { "id": "..." }, ... ] }
        ProviderId::Groq | ProviderId::Openai | ProviderId::Anthropic => v
            .get("data")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        // Gemini models.list: { "models": [ { "name": "models/x", "supportedGenerationMethods": [...] } ] }
        ProviderId::Google | ProviderId::Vertex => v
            .get("models")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter(|m| {
                        m.get("supportedGenerationMethods")
                            .and_then(Value::as_array)
                            .is_some_and(|a| {
                                a.iter().any(|x| x.as_str() == Some("generateContent"))
                            })
                    })
                    .filter_map(|m| {
                        m.get("name")
                            .and_then(Value::as_str)
                            .map(|n| n.strip_prefix("models/").unwrap_or(n).to_string())
                    })
                    .collect()
            })
            .unwrap_or_default(),
        // Ollama /api/tags: { "models": [ { "name": "llama3:latest" }, ... ] }
        ProviderId::Ollama => v
            .get("models")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("name").and_then(Value::as_str).map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Whether a model id is a chat/text-generation model gcm can use (D7.1). OpenAI
/// and Groq `/models` return non-text families (whisper/tts/dall-e/embeddings);
/// an exclude-list is safer than an include-list (new chat families aren't missed).
/// Gemini is already filtered in [`parse_models`]; Anthropic/Ollama pass through.
fn keep_chat_model(id: ProviderId, model: &str) -> bool {
    match id {
        ProviderId::Openai | ProviderId::Groq => {
            const EXCLUDE: &[&str] = &[
                "whisper",
                "tts",
                "dall-e",
                "dalle",
                "embedding",
                "embed",
                "moderation",
                "guard",
                "babbage",
                "davinci",
                "audio",
                "image",
                "rerank",
            ];
            let m = model.to_ascii_lowercase();
            !EXCLUDE.iter().any(|bad| m.contains(bad))
        }
        _ => true,
    }
}

/// A static per-provider model catalog used when the live fetch is unavailable.
/// Always includes the provider's `default_model` so the default is selectable
/// offline. These are discovery hints, not the resolved model (no ADR violation).
fn static_fallback_models(id: ProviderId) -> Vec<String> {
    let curated: &[&str] = match id {
        ProviderId::Groq => &[
            "openai/gpt-oss-120b",
            "openai/gpt-oss-20b",
            "llama-3.3-70b-versatile",
        ],
        ProviderId::Openai => &["gpt-5.4-mini", "gpt-5.4", "gpt-4o-mini"],
        ProviderId::Anthropic => &["claude-haiku-4-5", "claude-sonnet-4-6", "claude-opus-4-8"],
        ProviderId::Google | ProviderId::Vertex => &[
            "gemini-3.1-flash-lite",
            "gemini-3.1-flash",
            "gemini-3.1-pro",
        ],
        ProviderId::Ollama => &[],
    };
    let mut out: Vec<String> = curated.iter().map(|s| s.to_string()).collect();
    let default = id.default_model().to_string();
    if !out.contains(&default) {
        out.insert(0, default);
    }
    out
}

/// Stable de-duplication preserving first occurrence (live entries stay first).
fn dedupe(models: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    models
        .into_iter()
        .filter(|m| seen.insert(m.clone()))
        .collect()
}

/// Display name for error messages (mirrors each backend's `NAME`).
fn provider_name(id: ProviderId) -> &'static str {
    match id {
        ProviderId::Groq => "Groq",
        ProviderId::Google => "Google",
        ProviderId::Openai => "OpenAI",
        ProviderId::Anthropic => "Anthropic",
        ProviderId::Ollama => "Ollama",
        ProviderId::Vertex => "Vertex",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_openai_compatible_data_ids() {
        let body = r#"{"object":"list","data":[{"id":"gpt-5.4-mini"},{"id":"whisper-1"},{"id":"text-embedding-3-small"}]}"#;
        let ids = parse_models(ProviderId::Openai, body);
        assert_eq!(
            ids,
            vec!["gpt-5.4-mini", "whisper-1", "text-embedding-3-small"]
        );
    }

    #[test]
    fn parse_gemini_filters_generate_content_and_strips_prefix() {
        let body = r#"{"models":[
            {"name":"models/gemini-3.1-flash-lite","supportedGenerationMethods":["generateContent","countTokens"]},
            {"name":"models/text-embedding-004","supportedGenerationMethods":["embedContent"]}
        ]}"#;
        let ids = parse_models(ProviderId::Google, body);
        assert_eq!(
            ids,
            vec!["gemini-3.1-flash-lite"],
            "only generateContent, de-prefixed"
        );
    }

    #[test]
    fn parse_ollama_tags_names() {
        let body = r#"{"models":[{"name":"llama3:latest"},{"name":"gemma4:e4b-mlx"}]}"#;
        let ids = parse_models(ProviderId::Ollama, body);
        assert_eq!(ids, vec!["llama3:latest", "gemma4:e4b-mlx"]);
    }

    #[test]
    fn parse_malformed_body_is_empty() {
        assert!(parse_models(ProviderId::Openai, "not json [").is_empty());
        assert!(parse_models(ProviderId::Openai, "{}").is_empty());
    }

    #[test]
    fn keep_chat_model_excludes_non_text_for_openai_groq() {
        for bad in [
            "whisper-large-v3",
            "tts-1",
            "dall-e-3",
            "text-embedding-3-small",
            "omni-moderation-latest",
        ] {
            assert!(!keep_chat_model(ProviderId::Openai, bad), "{bad} excluded");
        }
        for good in [
            "gpt-5.4-mini",
            "gpt-4o",
            "openai/gpt-oss-120b",
            "llama-3.3-70b-versatile",
        ] {
            assert!(keep_chat_model(ProviderId::Groq, good), "{good} kept");
        }
        // Anthropic/Ollama pass through (no exclude-list)
        assert!(keep_chat_model(ProviderId::Anthropic, "claude-haiku-4-5"));
        assert!(keep_chat_model(ProviderId::Ollama, "anything:latest"));
    }

    #[test]
    fn fallback_always_contains_default_model() {
        for id in [
            ProviderId::Groq,
            ProviderId::Google,
            ProviderId::Openai,
            ProviderId::Anthropic,
            ProviderId::Ollama,
        ] {
            let fb = static_fallback_models(id);
            assert!(
                fb.iter().any(|m| m == id.default_model()),
                "{:?} fallback must include its default {}",
                id,
                id.default_model()
            );
        }
        // Ollama fallback is exactly its default (no cloud catalog).
        assert_eq!(
            static_fallback_models(ProviderId::Ollama),
            vec![ProviderId::Ollama.default_model().to_string()]
        );
    }

    #[test]
    fn no_key_short_circuits_to_fallback_without_network() {
        // A cloud provider with no key must not hit the network: returns fallback +
        // a warning naming its key env var. (No network is reachable in tests.)
        let out = fetch_supported_models(ProviderId::Openai, None, None);
        assert!(matches!(out.source, FetchSource::Fallback));
        assert!(out.warning.as_deref().unwrap().contains("OPENAI_API_KEY"));
        assert!(out
            .models
            .iter()
            .any(|m| m == ProviderId::Openai.default_model()));
    }

    #[test]
    fn dedupe_preserves_first_occurrence() {
        assert_eq!(
            dedupe(vec!["a".into(), "b".into(), "a".into(), "c".into()]),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn base_url_google_honors_gemini_then_google_alias() {
        // explicit endpoint wins
        assert_eq!(
            resolved_base_url_with(ProviderId::Ollama, Some("http://h:1"), |_| None),
            "http://h:1"
        );
        // Google primary GCM_GEMINI_BASE_URL wins over the GCM_GOOGLE_BASE_URL alias
        let g = resolved_base_url_with(ProviderId::Google, None, |v| match v {
            "GCM_GEMINI_BASE_URL" => Some("https://primary".to_string()),
            "GCM_GOOGLE_BASE_URL" => Some("https://alias".to_string()),
            _ => None,
        });
        assert_eq!(g, "https://primary");
        // the GCM_GOOGLE_BASE_URL alias is honored when the primary is unset
        let a = resolved_base_url_with(ProviderId::Google, None, |v| {
            (v == "GCM_GOOGLE_BASE_URL").then(|| "https://alias".to_string())
        });
        assert_eq!(a, "https://alias", "alias must be honored (review M1)");
        // default when neither is set
        assert_eq!(
            resolved_base_url_with(ProviderId::Google, None, |_| None),
            "https://generativelanguage.googleapis.com"
        );
    }
}
