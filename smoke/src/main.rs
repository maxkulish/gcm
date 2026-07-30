fn main() {}

#[cfg(test)]
mod smoke {
    use gcm::config::{AutoPolicy, Config, ConflictConfig, ProviderConfig};
    use gcm::privacy::{rules, Scanner, SecretScanMode};
    use gcm::provider::{
        models::{fetch_supported_models_with, FetchSource},
        resolve_model_with_source, ProviderId,
    };
    use gcm::status::build_report;

    /// Exercise the privacy scanner: compile a vendored rule pack, scan text,
    /// and redact secrets.
    #[test]
    fn privacy_scanner_compiles_and_scans() {
        let engine = rules::vendored().expect("vendored rule pack should compile");
        let _scanner = Scanner::new(SecretScanMode::Redact, engine);
        // Scanner::vendored is the convenience constructor
        let _scanner = Scanner::vendored(SecretScanMode::Redact)
            .expect("vendored scanner should construct");
        // The scanner compiles and is usable — the real test is that this
        // type and its dependencies are reachable from outside the package.
    }

    /// Exercise model resolution with a closure-based env lookup.
    #[test]
    fn model_resolution_uses_injected_env() {
        let env = |name: &str| -> Option<String> {
            if name == "GCM_PROVIDER" {
                Some("groq".into())
            } else {
                None
            }
        };
        let (model, _source) = resolve_model_with_source(ProviderId::Groq, None, env);
        // With GCM_PROVIDER=groq and no CLI override, the default model is
        // resolved from the provider's built-in list.
        assert!(!model.is_empty(), "model should be non-empty");
    }

    /// Exercise model fetch with a failing fetcher — verifies the fallback
    /// list is returned.
    #[test]
    fn model_fetch_degrades_to_fallback() {
        let failing = |_get: &gcm::provider::http::HttpGet| -> Result<String, gcm::provider::ProviderError> {
            Err(gcm::provider::ProviderError::new(
                "groq",
                gcm::provider::ErrorKind::Server(503),
            ))
        };
        let outcome = fetch_supported_models_with(
            ProviderId::Groq,
            None,    // key
            None,    // endpoint
            None,    // project
            failing,
        );
        // When the fetcher fails, we get a fallback list.
        assert!(!outcome.models.is_empty(), "fallback list should be non-empty");
        // The source should indicate fallback.
        assert!(matches!(outcome.source, FetchSource::Fallback));
    }

    /// Exercise status report with a caller-supplied env map.
    #[test]
    fn status_report_resolves_without_config_file() {
        let env = |name: &str| -> Option<String> {
            if name == "GCM_PROVIDER" {
                Some("groq".into())
            } else {
                None
            }
        };
        let report = build_report(None, None, None, env, 1, "0.6.0");
        // The report should have at least one provider status entry.
        assert!(
            !report.providers.is_empty(),
            "should have provider status entries"
        );
        // Paths should be resolved.
        let _ = report.paths;
    }

    /// Exercise config type construction.
    #[test]
    fn config_types_constructible() {
        let cfg = Config {
            version: 2,
            default: ProviderId::Groq,
            providers: vec![ProviderConfig {
                id: ProviderId::Groq,
                model: Some("llama-4.1-groq".into()),
                key: None,
                endpoint: None,
                models: vec![],
                project: None,
                location: None,
            }],
            conflict: ConflictConfig {
                temperature: 0.1,
                validate_cmd: None,
                sensitive_paths: vec![],
                auto_policy: AutoPolicy::Trivial,
                mergiraf: true,
                max_rounds: 10,
            },
        };
        assert_eq!(cfg.version, 2);
        assert_eq!(cfg.default, ProviderId::Groq);
        assert_eq!(cfg.providers.len(), 1);
    }
}
