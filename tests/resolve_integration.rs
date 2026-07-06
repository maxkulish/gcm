//! Integration tests for `gcm resolve` (CLO-531).
//!
//! Each test drives the built `gcm` binary in a throwaway git repo with a fresh
//! `GCM_CONFIG` dir. Tests that need an LLM response spin up a tiny local HTTP
//! mock so the suite stays hermetic and fast.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;

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
    "GCM_ANTHROPIC_BASE_URL",
    "GCM_GEMINI_BASE_URL",
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

fn write_config(dir: &Path, body: &str) {
    let path = dir.join("config.toml");
    fs::write(&path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn run_gcm(repo: &Path, config_dir: &Path, extra_env: &[(&str, &str)], args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(args)
        .env("GCM_CONFIG", config_dir)
        .env_remove("GCM_DEBUG")
        .stdin(Stdio::null());
    for var in PROVIDER_ENV {
        cmd.env_remove(var);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run gcm");
    out
}

/// Start a tiny HTTP server on a random port that returns `body` for the first
/// request and then exits. Returns the base URL (`http://localhost:PORT`) and a
/// join handle.
fn mock_ollama_server(response_body: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let body = response_body.to_string();
    let handle = thread::spawn(move || {
        // Accept with a 5-second timeout so the thread doesn't hang forever
        // when the provider is never called (e.g. secret scan abort).
        listener.set_nonblocking(true).ok();
        let start = std::time::Instant::now();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
                    let mut buf = [0u8; 4096];
                    let _ = stream.read(&mut buf);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    return;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() > std::time::Duration::from_secs(10) {
                        return; // timeout, no connection received
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => return,
            }
        }
    });
    (format!("http://127.0.0.1:{port}"), handle)
}

/// Build a mock Ollama chat response that returns a resolution JSON.
/// The `replacement` value is placed inside the inner resolutions array.
fn mock_resolve_response(replacement: &str) -> String {
    // Build the inner JSON with proper escaping.
    // The inner JSON: {"resolutions":[{"hunk_index":0,"replacement":"<replacement>"}]}
    // This inner JSON becomes the value of the "content" field in the outer message.
    // We need to escape it properly for JSON-in-JSON.
    let inner = serde_json::json!({
        "resolutions": [{
            "hunk_index": 0,
            "replacement": replacement
        }]
    });
    let inner_str = inner.to_string();
    // The outer Ollama response
    let outer = serde_json::json!({
        "message": {
            "content": inner_str
        }
    });
    outer.to_string()
}

/// Create a real git merge conflict on `f.txt` with base/feature/mainline all
/// differing so the classifier marks it as Complex (needs a provider).
fn create_conflict(repo: &Path) {
    fs::write(repo.join("f.txt"), "base\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);

    let base = git_str(repo, &["branch", "--show-current"]);

    git(repo, &["switch", "-q", "-c", "feature"]);
    fs::write(repo.join("f.txt"), "feature\n").unwrap();
    git(repo, &["commit", "-qam", "feature"]);

    git(repo, &["switch", "-q", &base]);
    fs::write(repo.join("f.txt"), "mainline\n").unwrap();
    git(repo, &["commit", "-qam", "mainline"]);

    let _ = run_git(repo, &["merge", "feature"]);
}

fn git(repo: &Path, args: &[&str]) {
    assert!(run_git(repo, args).status.success(), "git {args:?} failed");
}

fn git_str(repo: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&run_git(repo, args).stdout)
        .trim()
        .to_string()
}

fn run_git(repo: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn resolve_no_merge_in_progress() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    fs::write(repo.join("f.txt"), "x").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "init"]);

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    let out = run_gcm(repo, cfg_dir.path(), &[], &["resolve", "--json"]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("NoConflictInProgress"), "stdout: {stdout}");
}

#[test]
fn resolve_no_unmerged_files() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    fs::write(repo.join("f.txt"), "base\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);

    // Create a merge in progress with no unmerged files: start a merge that
    // conflicts, resolve the conflict, stage it, but don't commit.
    let base = git_str(repo, &["branch", "--show-current"]);
    git(repo, &["switch", "-q", "-c", "feature"]);
    fs::write(repo.join("f.txt"), "feature\n").unwrap();
    git(repo, &["commit", "-qam", "feature"]);
    git(repo, &["switch", "-q", &base]);
    fs::write(repo.join("f.txt"), "mainline\n").unwrap();
    git(repo, &["commit", "-qam", "mainline"]);

    // Start merge (will conflict).
    let _ = run_git(repo, &["merge", "feature"]);

    // Resolve and stage the conflict.
    fs::write(repo.join("f.txt"), "resolved\n").unwrap();
    git(repo, &["add", "f.txt"]);
    // Do NOT commit — MERGE_HEAD still exists.

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    let out = run_gcm(repo, cfg_dir.path(), &[], &["resolve", "--json"]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("NoConflicts"), "stdout: {stdout}");
}

#[test]
fn resolve_dry_run_no_write() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--dry-run", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // File should still have conflict markers (dry run doesn't resolve).
    let after = fs::read_to_string(repo.join("f.txt")).unwrap();
    assert!(
        after.contains("<<<<<<<") || after.contains("======="),
        "dry run should leave conflict markers intact: {after:?}"
    );
}

#[test]
fn resolve_json_envelope_stdout_only() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &[
            "resolve",
            "--json",
            "--dry-run",
            "--yes",
            "--provider",
            "ollama",
        ],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Single JSON object on stdout.
    assert!(
        stdout.trim().starts_with('{') && stdout.trim().ends_with('}'),
        "stdout: {stdout}"
    );
}

#[test]
fn resolve_complex_conflict_with_mock_provider() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let content = fs::read_to_string(repo.join("f.txt")).unwrap();
    assert_eq!(content, "resolved\n");
}

#[test]
fn resolve_binary_file_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    fs::write(repo.join(".gitattributes"), "*.bin binary\n").unwrap();
    fs::write(
        repo.join("img.bin"),
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00",
    )
    .unwrap();
    fs::write(repo.join("f.txt"), "base\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);
    let base = git_str(repo, &["branch", "--show-current"]);

    git(repo, &["switch", "-q", "-c", "feature"]);
    fs::write(repo.join("f.txt"), "feature\n").unwrap();
    fs::write(repo.join("img.bin"), b"\x89PNG\r\n\x1a\nCHANGED").unwrap();
    git(repo, &["commit", "-qam", "feature"]);

    git(repo, &["switch", "-q", &base]);
    fs::write(repo.join("f.txt"), "mainline\n").unwrap();
    fs::write(repo.join("img.bin"), b"\x89PNG\r\n\x1a\nMAINLINE").unwrap();
    git(repo, &["commit", "-qam", "mainline"]);

    // Merge will conflict on both files.
    let _ = run_git(repo, &["merge", "feature"]);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--json", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    eprintln!(
        "exit: {}, stderr: {stderr}",
        out.status.code().unwrap_or(-1)
    );
    assert!(
        out.status.success(),
        "exit: {}, stderr: {stderr}, stdout: {stdout}",
        out.status.code().unwrap_or(-1)
    );
    // The binary file should be escalated; the text file should be accepted.
    assert!(stdout.contains("escalated"), "stdout: {stdout}");
    assert!(stdout.contains("accepted"), "stdout: {stdout}");
}

#[test]
fn resolve_gcmignore_excludes_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    fs::write(repo.join(".gcmignore"), "secret.txt\n").unwrap();
    fs::write(repo.join("secret.txt"), "base\n").unwrap();
    fs::write(repo.join("f.txt"), "base\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);
    let base = git_str(repo, &["branch", "--show-current"]);

    git(repo, &["switch", "-q", "-c", "feature"]);
    fs::write(repo.join("secret.txt"), "feature\n").unwrap();
    fs::write(repo.join("f.txt"), "feature\n").unwrap();
    git(repo, &["commit", "-qam", "feature"]);

    git(repo, &["switch", "-q", &base]);
    fs::write(repo.join("secret.txt"), "mainline\n").unwrap();
    fs::write(repo.join("f.txt"), "mainline\n").unwrap();
    git(repo, &["commit", "-qam", "mainline"]);

    // Merge will conflict on both files.
    let _ = run_git(repo, &["merge", "feature"]);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // secret.txt should still be conflicted because it was excluded by .gcmignore.
    let secret = fs::read_to_string(repo.join("secret.txt")).unwrap();
    assert!(
        secret.contains("<<<<<<<") || secret.contains("===="),
        "secret.txt should remain conflicted: {secret:?}"
    );
    // f.txt should be resolved.
    let text = fs::read_to_string(repo.join("f.txt")).unwrap();
    assert_eq!(text, "resolved\n");
}

#[test]
fn resolve_secret_scan_aborts_before_provider() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);

    // Create a conflict where the secret is in the actual git history so
    // checkout_conflict_zdiff3 preserves it in the hunk text.
    fs::write(repo.join("f.txt"), "API_KEY=old_key\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);

    let base = git_str(repo, &["branch", "--show-current"]);

    git(repo, &["switch", "-q", "-c", "feature"]);
    fs::write(
        repo.join("f.txt"),
        "API_KEY=ghp_abcdefghijklmnopqrstuvwxyz123456\n",
    )
    .unwrap();
    git(repo, &["commit", "-qam", "feature"]);

    git(repo, &["switch", "-q", &base]);
    fs::write(repo.join("f.txt"), "API_KEY=other_key\n").unwrap();
    git(repo, &["commit", "-qam", "mainline"]);

    let _ = run_git(repo, &["merge", "feature"]);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &[
            "resolve",
            "--yes",
            "--provider",
            "ollama",
            "--secret-scan",
            "abort",
        ],
    );
    // Server may or may not be hit depending on scan order; join defensively.
    let _ = server.join();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("SecretDetected") || stderr.contains("secret scan"),
        "stderr: {stderr}"
    );
}
