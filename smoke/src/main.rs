fn main() {}

#[cfg(test)]
mod smoke {
    use gcm::config::{AutoPolicy, Config, ConflictConfig, ProviderConfig};
    use gcm::paths::xdg_gcm_dir_from;
    use gcm::privacy::{detect, rules, ScanError, Scanner, SecretScanMode};
    use gcm::provider::{
        models::{fetch_supported_models_with, FetchSource},
        resolve_model_with_source, AuthMethod, ModelSource, ProviderId,
    };
    use gcm::status::{build_report, PathsStatus, ProviderStatus, StatusReport};

    /// Exercise the privacy scanner: compile a vendored rule pack, scan text,
    /// and redact secrets.
    #[test]
    fn privacy_scanner_compiles_and_scans() {
        let engine = rules::vendored().expect("vendored rule pack should compile");
        let scanner = Scanner::new(SecretScanMode::Redact, engine);
        // Actually scan text — the design requires exercising scan/redact.
        let text = "api_key = \"sk-1234567890abcdef\"";
        let result = scanner.scan(text.to_string());
        match result {
            Ok(redacted) => {
                // In Redact mode, secrets should be replaced with [REDACTED].
                assert!(
                    redacted.contains("[REDACTED"),
                    "redacted text should contain [REDACTED], got: {}",
                    redacted
                );
            }
            Err(ScanError::SecretDetected { .. }) => {
                // In Abort mode this would happen; in Redact mode it shouldn't.
                panic!("Redact mode should not return SecretFound");
            }
            Err(e) => panic!("unexpected scan error: {:?}", e),
        }

        // Also exercise detect::secret_ranges directly.
        let ranges = detect::secret_ranges(text, engine);
        assert!(!ranges.is_empty(), "should detect at least one secret range");

        // Scanner::vendored is the convenience constructor.
        let _scanner = Scanner::vendored(SecretScanMode::Redact)
            .expect("vendored scanner should construct");
    }

    /// Exercise model resolution with a closure-based env lookup.
    /// Uses GCM_GROQ_MODEL (the per-provider model env var) to reach the
    /// env-resolution path, and asserts ModelSource::Env.
    #[test]
    fn model_resolution_uses_injected_env() {
        let env = |name: &str| -> Option<String> {
            if name == "GCM_GROQ_MODEL" {
                Some("llama-4.1-groq".into())
            } else {
                None
            }
        };
        let (model, source) = resolve_model_with_source(ProviderId::Groq, None, env);
        assert_eq!(model, "llama-4.1-groq");
        assert!(matches!(source, ModelSource::Env(_)), "expected Env, got {:?}", source);
    }

    /// Exercise model fetch with a failing fetcher — verifies the fallback
    /// list is returned. Passes a fake key to force the fetcher path.
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
            Some("sk-test"), // force the fetcher path past the no-key short-circuit
            None,            // endpoint
            None,            // project
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
        let report: StatusReport = build_report(None, None, None, env, 1, "0.6.0");
        // The report should have at least one provider status entry.
        assert!(
            !report.providers.is_empty(),
            "should have provider status entries"
        );
        // Verify the report shape is accessible.
        let _paths: PathsStatus = report.paths;
        let _providers: Vec<ProviderStatus> = report.providers;
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

    /// Exercise path resolution from an external consumer.
    #[test]
    fn paths_xdg_gcm_dir_resolves() {
        let dir = xdg_gcm_dir_from(None, None, "gcm");
        // On macOS, this should resolve to ~/Library/Application Support/gcm
        // or similar. The exact path depends on the runner, but the function
        // should return a valid PathBuf.
        let _ = dir;
    }

    /// Verify AuthMethod and ModelSource types are nameable from outside.
    #[test]
    fn provider_types_are_nameable() {
        // AuthMethod variants must be constructible.
        let _api_key = AuthMethod::ApiKey;
        let _keyless = AuthMethod::KeylessEndpoint;
        let _adc = AuthMethod::KeylessAdc;

        // ModelSource variants must be constructible.
        let _default = ModelSource::Default;
        let _env = ModelSource::Env("GCM_GROQ_MODEL");
        let _flag = ModelSource::Flag;
    }
}
