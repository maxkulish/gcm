//! Library consumer integration test for CLO-596.
//!
//! This test exercises the public `gcm::provider` API from outside the crate,
//! using only injected dependencies (no process env, no network).

use gcm::provider::{
    models::{fetch_supported_models_with, FetchSource, ModelFetchOutcome},
    resolve_model_with_source, ModelSource, ProviderId,
};

#[test]
fn library_resolve_model_with_source_uses_env_lookup() {
    let (model, source) = resolve_model_with_source(ProviderId::Groq, None, |v| {
        if v == "GCM_GROQ_MODEL" {
            Some("env-model".to_string())
        } else {
            None
        }
    });
    assert_eq!(model, "env-model");
    assert_eq!(source, ModelSource::Env("GCM_GROQ_MODEL"));
}

#[test]
fn library_fetch_supported_models_with_uses_injected_fetcher() {
    let outcome =
        fetch_supported_models_with(ProviderId::Groq, Some("sk-123"), None, None, |_req| {
            Ok(r#"{"data":[{"id":"llama-3.3-70b-versatile"},{"id":"whisper-1"}]}"#.to_string())
        });
    assert_eq!(outcome.models, vec!["llama-3.3-70b-versatile"]);
    assert!(matches!(outcome.source, FetchSource::Live));
    assert!(outcome.warning.is_none());
}

#[test]
fn library_fetch_supported_models_with_degrades_to_fallback_on_error() {
    let outcome: ModelFetchOutcome =
        fetch_supported_models_with(ProviderId::Groq, Some("sk-123"), None, None, |_req| {
            Err(gcm::provider::ProviderError::new(
                "Groq",
                gcm::provider::ErrorKind::Http(503),
            ))
        });
    assert!(
        !outcome.models.is_empty(),
        "fallback list must be non-empty"
    );
    assert!(matches!(outcome.source, FetchSource::Fallback));
    assert!(outcome.warning.is_some());
}

#[test]
fn library_provider_id_parse_is_clap_free() {
    // Exact aliases that the CLI also accepts.
    assert_eq!(ProviderId::parse("gemini"), Some(ProviderId::Google));
    assert_eq!(ProviderId::parse("google-vertex"), Some(ProviderId::Vertex));
    assert_eq!(ProviderId::parse("  GoOgLe "), Some(ProviderId::Google));
    assert_eq!(ProviderId::parse("bogus"), None);
}
