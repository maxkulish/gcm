//! Integration tests that exercise the public `gcm` privacy API as an external
//! consumer would.

use std::collections::HashMap;

use gcm::privacy::{detect, rules, ScanError, Scanner, SecretScanMode};

#[test]
fn external_consumer_compiles_engine_and_scans_text() {
    let engine = rules::vendored().expect("vendored rule pack parses");
    let text = "token=ghp_abcdefghijklmnopqrstuvwxyz123456\nAWS=AKIAABCDEFGHIJKLMNOP\n";

    let ranges = detect::secret_ranges(text, engine);
    assert!(!ranges.is_empty());

    let redacted = detect::redact_secrets(text, engine);
    assert!(!redacted.contains("ghp_abcdefghijklmnopqrstuvwxyz123456"));
    assert!(!redacted.contains("AKIAABCDEFGHIJKLMNOP"));
}

#[test]
fn abort_mode_from_caller_supplied_env_map() {
    let env = HashMap::from([("GCM_SECRET_SCAN".to_string(), "abort".to_string())]);
    let mode = SecretScanMode::resolve_with(None, |k| env.get(k).cloned())
        .expect("mode resolves from provided map");
    assert_eq!(mode, SecretScanMode::Abort);

    let engine = rules::vendored().expect("vendored rule pack parses");
    let scanner = Scanner::new(mode, engine);
    let err = scanner.scan("API_KEY=supersecret12345\n".to_string());

    assert!(matches!(err, Err(ScanError::SecretDetected { count: 1 })));
}

#[test]
fn custom_rule_pack_compiles_and_matches() {
    let pack = r#"
[[rules]]
id = "custom_api_key"
keywords = ["my_api_key"]
regex = "(?i)api[_-]?key\\s*=\\s*[A-Za-z0-9]{10,}"
min_length = 8
entropy = 2.5
"#;

    let engine = rules::RuleEngine::compile(pack).expect("custom pack compiles");
    let text = "my_api_key=abcd1234EFGH\n";
    let ranges = detect::secret_ranges(text, &engine);
    assert_eq!(ranges.len(), 1);
}

#[test]
fn scan_error_is_std_error() {
    fn accepts_error(_: &dyn std::error::Error) {}

    let err: ScanError = ScanError::InvalidMode {
        value: "panic".to_string(),
    };
    accepts_error(&err);
}
