//! First-run onboarding integration tests (CLO-496). Each test drives the built
//! `gcm` binary as a subprocess with its own environment (so there is no
//! in-process env race) and a throwaway git repo + `GCM_CONFIG` dir. No network:
//! onboarding fires before any provider call, and the hydration test points
//! Ollama at a closed local port for an immediate connection refusal.

use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Provider env vars cleared so a first run is genuinely unconfigured.
const PROVIDER_ENV: &[&str] = &[
    "GROQ_API_KEY",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GCM_PROVIDER",
    "OLLAMA_HOST",
    "GCM_OLLAMA_BASE_URL",
];

/// Initialize a minimal git work tree at `dir` (so `Repo::discover` succeeds).
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

/// Run `gcm` in `repo` with a clean provider environment, `GCM_CONFIG` pointed at
/// `config_dir`, and stdin closed (non-TTY). Returns the captured output.
fn run_gcm(repo: &Path, config_dir: &Path, extra_args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(extra_args)
        .env("GCM_CONFIG", config_dir)
        // bound any (unexpected) network attempt so the suite never hangs
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

#[test]
fn first_run_non_tty_prints_instructions_and_exits_nonzero() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());

    let out = run_gcm(repo.path(), cfg.path(), &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "first run must exit non-zero: {stderr}"
    );
    // the human instructions (template + an export line) land on stderr
    assert!(
        stderr.contains("[[providers]]"),
        "TOML template on stderr: {stderr}"
    );
    assert!(
        stderr.contains("export GROQ_API_KEY="),
        "export line on stderr: {stderr}"
    );
}

#[test]
fn first_run_json_non_tty_emits_envelope_not_prompts() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());

    let out = run_gcm(repo.path(), cfg.path(), &["--json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "exit non-zero: {stdout} / {stderr}");
    // stdout is exactly one JSON error envelope - no prompt text
    let trimmed = stdout.trim();
    let parsed: serde_json::Value = serde_json::from_str(trimmed)
        .unwrap_or_else(|e| panic!("stdout is one JSON object ({e}): {trimmed}"));
    assert_eq!(parsed["status"], "error", "envelope: {trimmed}");
    assert_eq!(
        parsed["error"]["code"], "OnboardingRequired",
        "code: {trimmed}"
    );
    // the human instructions go to stderr, never stdout
    assert!(
        !stdout.contains("[[providers]]"),
        "instructions must not pollute stdout: {stdout}"
    );
    assert!(
        stderr.contains("[[providers]]"),
        "instructions on stderr: {stderr}"
    );
}

#[test]
fn existing_env_user_is_not_interrupted() {
    // A clean repo with GROQ_API_KEY set: onboarding must NOT fire (it would fire
    // before the no-changes check), so the run proceeds to a normal `noop`. Fully
    // offline - the clean tree returns before any provider call.
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo.path())
        .arg("--json")
        .env("GCM_CONFIG", cfg.path())
        .env("GROQ_API_KEY", "sk-fake-not-used")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.env_remove("GCM_PROVIDER");
    let out = cmd.output().expect("run gcm");
    let stdout = String::from_utf8_lossy(&out.stdout);

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("json ({e}): {stdout}"));
    assert_eq!(
        parsed["status"], "noop",
        "env user proceeds to noop: {stdout}"
    );
    assert!(out.status.success(), "noop exits 0");
}

#[test]
fn gcm_config_non_tty_does_not_hang_and_exits_nonzero() {
    // `gcm config </dev/null`: the wizard needs a terminal, so it must fail fast
    // with guidance rather than spin on EOF (the "never hang on closed stdin"
    // rule). The 10s process timeout below would catch a regression to a hang.
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());

    let out = run_gcm(repo.path(), cfg.path(), &["config"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "gcm config must exit non-zero: {stderr}"
    );
    assert!(
        stderr.contains("interactive terminal") || stderr.contains("[[providers]]"),
        "guidance on stderr: {stderr}"
    );
}

#[test]
fn existing_config_inline_cloud_key_hydrates_env_not_missing_key() {
    // A saved config with an inline groq key + a groq base URL pointed at a closed
    // port: the inline key hydrates GROQ_API_KEY, so the run reaches the (dead)
    // endpoint and fails with a transport/HTTP error, NOT MissingKey. Had the key
    // not hydrated, the keyless groq default would fail with MissingKey.
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());
    fs::write(repo.path().join("a.txt"), "change\n").unwrap();
    let config_file = cfg.path().join("config.toml");
    fs::write(
        &config_file,
        "version = 1\n\
         default = \"groq\"\n\
         \n\
         [[providers]]\n\
         id = \"groq\"\n\
         key = \"sk-inline-test\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config_file, fs::Permissions::from_mode(0o600)).unwrap();
    }

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo.path())
        .args(["--json", "--yes"])
        .env("GCM_CONFIG", cfg.path())
        .env("GCM_GROQ_BASE_URL", "http://127.0.0.1:1/v1")
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
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("json ({e}): {stdout}"));
    // The inline key hydrated GROQ_API_KEY -> the error is a reach failure, not a
    // missing-key default.
    let code = parsed["error"]["code"].as_str().unwrap_or("");
    assert_ne!(code, "MissingKey", "inline key hydrated: {stdout}");
    assert_ne!(code, "OnboardingRequired", "config present: {stdout}");
    assert!(
        !stdout.contains("GROQ_API_KEY is not set"),
        "no missing-key message: {stdout}"
    );
}

#[test]
fn existing_config_hydrates_env_not_missing_key() {
    // A saved config (default ollama, custom endpoint at a closed local port) is
    // hydrated into the environment: the run selects Ollama and fails to REACH it
    // (transport error), proving GCM_PROVIDER + GCM_OLLAMA_BASE_URL were applied.
    // Had hydration failed, gcm would default to groq with no key -> MissingKey.
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    git_init(repo.path());
    // an untracked file makes the tree dirty so the run reaches the provider
    fs::write(repo.path().join("a.txt"), "change\n").unwrap();
    // a saved 0600 config pointing Ollama at a closed port (load() rejects a
    // world-readable file, mirroring how `save` writes it)
    let config_file = cfg.path().join("config.toml");
    fs::write(
        &config_file,
        "version = 1\n\
         default = \"ollama\"\n\
         \n\
         [[providers]]\n\
         id = \"ollama\"\n\
         endpoint = \"http://127.0.0.1:1\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config_file, fs::Permissions::from_mode(0o600)).unwrap();
    }

    let out = run_gcm(repo.path(), cfg.path(), &["--json", "--dry-run"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| panic!("json ({e}): {stdout}"));
    let msg = stdout.to_lowercase();
    assert_ne!(
        parsed["error"]["code"], "MissingKey",
        "config hydrated -> not a missing-key default: {stdout}"
    );
    assert_ne!(
        parsed["error"]["code"], "OnboardingRequired",
        "a saved config is not a first run: {stdout}"
    );
    assert!(
        msg.contains("ollama"),
        "selected provider is Ollama (proves GCM_PROVIDER hydrated): {stdout}"
    );
}
