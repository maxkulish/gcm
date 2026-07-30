//! Library consumer test for `gcm::status` (CLO-597).
//!
//! Verifies that an external crate can resolve a full status report through the
//! library API using only a caller-supplied env map, with no gcm config file on
//! disk and no CLI dependencies.

use gcm::config::{Config, ProviderConfig};
use gcm::provider::ProviderId;
use gcm::status::build_report;
use std::collections::HashMap;

fn env_lookup(map: &HashMap<&str, &str>, name: &str) -> Option<String> {
    map.get(name).map(|&v| v.to_string())
}

#[test]
fn library_status_api_resolves_report_without_config_file() {
    let env = HashMap::from([
        ("GCM_PROVIDER", "openai"),
        ("OPENAI_API_KEY", "sk-lib-test"),
        ("GCM_OPENAI_MODEL", "gpt-lib"),
    ]);

    let report = build_report(
        None,
        None,
        None,
        |name| env_lookup(&env, name),
        1,
        "lib-test",
    );

    assert_eq!(report.v, 1);
    assert_eq!(report.version, "lib-test");

    let openai = report
        .providers
        .iter()
        .find(|p| p.name == ProviderId::Openai)
        .expect("openai provider present");
    assert!(openai.selected, "GCM_PROVIDER=openai selects openai");
    assert_eq!(openai.model, "gpt-lib");
    assert_eq!(openai.model_source, "env var GCM_OPENAI_MODEL");
    assert_eq!(openai.key_source.as_deref(), Some("env var OPENAI_API_KEY"));

    // Every provider appears in canonical order.
    let names: Vec<&str> = report.providers.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["groq", "google", "vertex", "openai", "anthropic", "ollama"]
    );
}

#[test]
fn library_status_api_uses_config_file_model_when_env_unset() {
    let config = Config {
        version: 2,
        default: ProviderId::Groq,
        providers: vec![ProviderConfig {
            id: ProviderId::Groq,
            key: None,
            endpoint: None,
            model: Some("groq-config-model".to_string()),
            models: Vec::new(),
            project: None,
            location: None,
        }],
        conflict: Default::default(),
    };

    let env = HashMap::from([("GROQ_API_KEY", "sk-groq")]);

    let report = build_report(
        None,
        None,
        Some(&config),
        |name| env_lookup(&env, name),
        2,
        "lib-test-2",
    );

    let groq = report
        .providers
        .iter()
        .find(|p| p.name == ProviderId::Groq)
        .unwrap();
    assert!(groq.selected);
    assert_eq!(groq.model, "groq-config-model");
    assert_eq!(groq.model_source, "config file");
    assert_eq!(groq.key_source.as_deref(), Some("env var GROQ_API_KEY"));
}
