//! Integration tests for the `gcm provider` enabled-model whitelist + the v1->v2
//! config migration (CLO-516). Each drives the built `gcm` binary as a subprocess
//! with its own throwaway git repo + `GCM_CONFIG` dir and a cleared provider
//! environment, so the on-disk config is the only driver. No network: enforcement
//! and migration run before any provider call, and base URLs point at a closed
//! local port for an immediate refusal where a request would otherwise happen.
//!
//! The interactive cliclack wizard itself (AC-1/AC-3) reads `/dev/tty` and is
//! verified manually; here we cover everything reachable without a TTY: the
//! non-TTY guard, migration, and runtime enforcement (incl. the clean-repo timing).

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

const PROVIDER_ENV: &[&str] = &[
    "GROQ_API_KEY",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GCM_PROVIDER",
    "OLLAMA_HOST",
    "GCM_OLLAMA_BASE_URL",
    "GCM_OPENAI_BASE_URL",
    "GCM_GROQ_BASE_URL",
];

fn git_init(dir: &Path) {
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "t@example.com"],
        vec!["config", "user.name", "Test"],
    ] {
        let ok = Command::new("git")
            .args(&args)
            .current_dir(dir)
            .status()
            .expect("run git")
            .success();
        assert!(ok, "git {args:?} failed");
    }
}

/// Write a `0600` config file (load() rejects a world-readable one).
fn write_config(dir: &Path, body: &str) {
    let path = dir.join("config.toml");
    fs::write(&path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

/// Run `gcm` in `repo` with a cleared provider env, `GCM_CONFIG` at `config_dir`,
/// stdin closed (non-TTY), and bounded network so the suite never hangs.
fn run_gcm(repo: &Path, config_dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(args)
        .env("GCM_CONFIG", config_dir)
        .env("GCM_HTTP_TIMEOUT_SECS", "2")
        .env("GCM_RETRY_MAX", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for var in PROVIDER_ENV {
        cmd.env_remove(var);
    }
    cmd.output().expect("run gcm")
}

fn error_code(stdout: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("json ({e}): {stdout}"));
    parsed["error"]["code"].as_str().unwrap_or("").to_string()
}

#[test]
fn provider_subcommand_non_tty_fails_with_guidance() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());

    let out = run_gcm(repo.path(), cfg.path(), &["provider"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "must exit non-zero: {stderr}");
    assert!(
        stderr.contains("interactive terminal") || stderr.contains("[[providers]]"),
        "guidance on stderr: {stderr}"
    );
}

#[test]
fn v1_config_loads_after_version_bump() {
    // A pre-CLO-516 v1 config must still hydrate after the bump (migration), not be
    // treated as a first run. Inline groq key + a closed base URL -> the run reaches
    // the dead endpoint and fails with a reach error, never OnboardingRequired.
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());
    fs::write(repo.path().join("a.txt"), "x\n").unwrap();
    write_config(
        cfg.path(),
        "version = 1\ndefault = \"groq\"\n\n[[providers]]\nid = \"groq\"\nkey = \"sk-inline\"\n",
    );

    let out = run_gcm(
        repo.path(),
        cfg.path(),
        &["--json", "--yes", "--provider", "groq"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let code = error_code(&stdout);
    assert_ne!(
        code, "OnboardingRequired",
        "v1 migrated, not a first run: {stdout}"
    );
    assert_ne!(code, "MissingKey", "inline key hydrated: {stdout}");
}

#[test]
fn enabled_model_outside_set_is_rejected() {
    // A non-empty `models` whitelist rejects an out-of-set `--model` with code Config.
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());
    fs::write(repo.path().join("a.txt"), "x\n").unwrap();
    write_config(
        cfg.path(),
        "version = 2\ndefault = \"openai\"\n\n[[providers]]\nid = \"openai\"\nkey = \"sk-x\"\nmodel = \"gpt-x\"\nmodels = [\"gpt-x\"]\n",
    );

    let out = run_gcm(
        repo.path(),
        cfg.path(),
        &["--json", "--yes", "--model", "dall-e-3"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        error_code(&stdout),
        "Config",
        "out-of-set model rejected: {stdout}"
    );
    assert!(
        stdout.contains("dall-e-3"),
        "names the offending model: {stdout}"
    );
    assert!(!out.status.success());
}

#[test]
fn empty_models_allows_any_model() {
    // An empty `models` (v1 migration / pre-wizard) is unrestricted: a free-form
    // `--model` is allowed, so the run proceeds past enforcement and fails later at
    // the (closed) endpoint - never with code Config.
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());
    fs::write(repo.path().join("a.txt"), "x\n").unwrap();
    write_config(
        cfg.path(),
        "version = 2\ndefault = \"openai\"\n\n[[providers]]\nid = \"openai\"\nkey = \"sk-x\"\n",
    );

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo.path())
        .args(["--json", "--yes", "--model", "anything-goes"])
        .env("GCM_CONFIG", cfg.path())
        .env("GCM_OPENAI_BASE_URL", "http://127.0.0.1:1/v1")
        .env("GCM_HTTP_TIMEOUT_SECS", "2")
        .env("GCM_RETRY_MAX", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for var in PROVIDER_ENV {
        cmd.env_remove(var);
    }
    let out = cmd.output().expect("run gcm");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_ne!(
        error_code(&stdout),
        "Config",
        "empty models = unrestricted: {stdout}"
    );
}

#[test]
fn enforcement_runs_on_clean_repo() {
    // Enforcement fires in ensure_configured, before the no-changes check, so a
    // clean/no-op repo with an out-of-set model errors (Config) rather than noop.
    // Intentional + consistent with onboarding's pre-check timing (review pt 13).
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path()); // no changes -> clean tree
    write_config(
        cfg.path(),
        "version = 2\ndefault = \"openai\"\n\n[[providers]]\nid = \"openai\"\nkey = \"sk-x\"\nmodel = \"gpt-x\"\nmodels = [\"gpt-x\"]\n",
    );

    let out = run_gcm(repo.path(), cfg.path(), &["--json", "--model", "dall-e-3"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        error_code(&stdout),
        "Config",
        "clean repo still enforces (pre-check): {stdout}"
    );
    assert!(!out.status.success());
}
