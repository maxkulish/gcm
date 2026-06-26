//! `gcm status` integration tests (CLO-515). Each test drives the built binary as
//! a subprocess with a cleared provider environment and a throwaway `GCM_CONFIG`
//! dir, so attribution is deterministic and hermetic. `gcm status` is read-only:
//! it needs no git repo and makes no network/LLM call, so tests run in a plain
//! temp dir and never hang.

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Every provider/model/endpoint env var cleared so attribution starts from a
/// known-empty baseline; individual tests re-add only what they assert on.
const CLEARED_ENV: &[&str] = &[
    "GROQ_API_KEY",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GCM_PROVIDER",
    "OLLAMA_HOST",
    "GCM_OLLAMA_BASE_URL",
    "GCM_GROQ_MODEL",
    "GCM_GEMINI_MODEL",
    "GCM_GOOGLE_MODEL",
    "GCM_OPENAI_MODEL",
    "GCM_ANTHROPIC_MODEL",
    "GCM_OLLAMA_MODEL",
];

/// Run `gcm` with a cleared provider env, `GCM_CONFIG` pointed at `config_dir`,
/// plus any `extra_env` (name, value) pairs. Returns the captured output.
fn run_status(config_dir: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.args(args)
        .env("GCM_CONFIG", config_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for var in CLEARED_ENV {
        cmd.env_remove(var);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.output().expect("run gcm status")
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Write a 0600 config.toml into `dir` (the GCM_CONFIG dir).
fn write_config(dir: &Path, body: &str) {
    let path = dir.join("config.toml");
    fs::write(&path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn status_no_config_clean_env_exits_zero() {
    let cfg = tempfile::tempdir().unwrap();
    let out = run_status(cfg.path(), &["status"], &[]);
    assert!(out.status.success(), "exit 0");
    let stdout = stdout_of(&out);
    assert!(stdout.contains("no config file"), "{stdout}");
    // every cloud provider with no key -> not activated, not set
    assert!(!stdout.contains("groq [selected, activated]"), "{stdout}");
    assert!(stdout.contains("not set"), "{stdout}");
    // default models surface
    assert!(stdout.contains("openai/gpt-oss-120b (default)"), "{stdout}");
    assert!(
        stdout.contains("gemini-3.1-flash-lite (default)"),
        "{stdout}"
    );
}

#[test]
fn status_env_key_and_model_attribution() {
    let cfg = tempfile::tempdir().unwrap();
    let out = run_status(
        cfg.path(),
        &["status"],
        &[("GROQ_API_KEY", "sk-secret123"), ("GCM_GROQ_MODEL", "m-x")],
    );
    assert!(out.status.success());
    let stdout = stdout_of(&out);
    assert!(stdout.contains("key:   env var GROQ_API_KEY"), "{stdout}");
    assert!(
        stdout.contains("model: m-x (env var GCM_GROQ_MODEL)"),
        "{stdout}"
    );
}

#[test]
fn status_never_prints_raw_secret() {
    let cfg = tempfile::tempdir().unwrap();
    // human mode
    let out = run_status(cfg.path(), &["status"], &[("GROQ_API_KEY", "sk-secret123")]);
    assert!(
        !stdout_of(&out).contains("sk-secret123"),
        "human leaked secret"
    );
    // json mode
    let out = run_status(
        cfg.path(),
        &["status", "--json"],
        &[("GROQ_API_KEY", "sk-secret123")],
    );
    assert!(
        !stdout_of(&out).contains("sk-secret123"),
        "json leaked secret"
    );
}

#[test]
fn status_mixed_inline_and_env_key_attribution() {
    let cfg = tempfile::tempdir().unwrap();
    // groq has an inline key; openai relies on the env var
    write_config(
        cfg.path(),
        "version = 1\n\
         default = \"groq\"\n\
         \n\
         [[providers]]\n\
         id = \"groq\"\n\
         key = \"sk-inline-xyz\"\n\
         \n\
         [[providers]]\n\
         id = \"openai\"\n",
    );
    let out = run_status(
        cfg.path(),
        &["status", "--json"],
        &[("OPENAI_API_KEY", "sk-env-openai")],
    );
    assert!(out.status.success());
    let stdout = stdout_of(&out);
    assert!(
        !stdout.contains("sk-inline-xyz"),
        "inline key leaked: {stdout}"
    );
    assert!(
        !stdout.contains("sk-env-openai"),
        "env key leaked: {stdout}"
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let providers = json["providers"].as_array().unwrap();
    let groq = providers.iter().find(|p| p["name"] == "groq").unwrap();
    let openai = providers.iter().find(|p| p["name"] == "openai").unwrap();
    assert_eq!(groq["key_source"], "config file");
    assert_eq!(openai["key_source"], "env var OPENAI_API_KEY");
    // config.default = groq -> groq is the selected provider
    assert_eq!(groq["selected"], true);
}

#[test]
fn status_json_valid_both_flag_positions() {
    let cfg = tempfile::tempdir().unwrap();
    for args in [["status", "--json"], ["--json", "status"]] {
        let out = run_status(cfg.path(), &args, &[]);
        assert!(out.status.success(), "exit 0 for {args:?}");
        let json: serde_json::Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("invalid JSON for {args:?}: {e}"));
        assert_eq!(json["v"], 1);
        assert_eq!(json["providers"].as_array().unwrap().len(), 5);
    }
}

#[test]
fn status_ollama_endpoint_source() {
    let cfg = tempfile::tempdir().unwrap();
    let out = run_status(cfg.path(), &["status"], &[("OLLAMA_HOST", "remote:8080")]);
    assert!(out.status.success());
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains("endpoint: http://remote:8080 (env var OLLAMA_HOST)"),
        "{stdout}"
    );
}

#[test]
fn status_model_flag_scoped_to_selected_provider() {
    let cfg = tempfile::tempdir().unwrap();
    // --provider openai --model custom: only openai reports flag
    let out = run_status(
        cfg.path(),
        &["--provider", "openai", "--model", "custom-model", "status"],
        &[],
    );
    assert!(out.status.success());
    let stdout = stdout_of(&out);
    assert!(stdout.contains("openai [selected"), "{stdout}");
    assert!(stdout.contains("model: custom-model (flag)"), "{stdout}");
    // other providers keep their defaults
    assert!(stdout.contains("claude-haiku-4-5 (default)"), "{stdout}");
}

#[test]
fn status_invalid_gcm_provider_reported_exit_zero() {
    let cfg = tempfile::tempdir().unwrap();
    let out = run_status(
        cfg.path(),
        &["status", "--json"],
        &[("GCM_PROVIDER", "bogus")],
    );
    assert!(out.status.success(), "invalid provider is not fatal");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let err = json["provider_error"].as_str().expect("provider_error set");
    assert!(err.contains("bogus"), "{err}");
    // falls back to groq as the displayed selection
    let groq = json["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "groq")
        .unwrap();
    assert_eq!(groq["selected"], true);
}

#[test]
fn status_malformed_config_falls_back_to_env_state() {
    let cfg = tempfile::tempdir().unwrap();
    write_config(cfg.path(), "this is not = valid toml [");
    let out = run_status(cfg.path(), &["status", "--json"], &[]);
    assert!(out.status.success(), "malformed config is not fatal");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["v"], 1);
    assert_eq!(json["providers"].as_array().unwrap().len(), 5);
}

#[test]
fn status_config_default_drives_selection_without_env() {
    let cfg = tempfile::tempdir().unwrap();
    write_config(
        cfg.path(),
        "version = 1\n\
         default = \"openai\"\n\
         \n\
         [[providers]]\n\
         id = \"openai\"\n",
    );
    // no --provider, no GCM_PROVIDER -> config.default (openai) is selected
    let out = run_status(cfg.path(), &["status", "--json"], &[]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let openai = json["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "openai")
        .unwrap();
    assert_eq!(
        openai["selected"], true,
        "config.default drives selection: {json}"
    );
}

#[test]
fn status_google_dual_env_precedence() {
    let cfg = tempfile::tempdir().unwrap();
    let out = run_status(
        cfg.path(),
        &["status", "--json"],
        &[
            ("GCM_GEMINI_MODEL", "gem-a"),
            ("GCM_GOOGLE_MODEL", "goog-b"),
        ],
    );
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let google = json["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "google")
        .unwrap();
    assert_eq!(google["model"], "gem-a");
    assert_eq!(google["model_source"], "env var GCM_GEMINI_MODEL");
}
