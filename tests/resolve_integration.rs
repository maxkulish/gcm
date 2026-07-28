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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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
    cmd.output().expect("run gcm")
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
                    // The listener is non-blocking so the accept loop can time out. On
                    // BSD/macOS the accepted socket inherits O_NONBLOCK from it (Linux
                    // does not), which makes set_read_timeout a no-op and lets read()
                    // and write_all() return WouldBlock before the request arrives.
                    // Restore blocking so the timeout below is what bounds the wait.
                    let _ = stream.set_nonblocking(false);
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

/// Serve `responses` in order, one per incoming request, and count how many
/// requests actually arrived.
///
/// The counter is what makes "this round spent nothing" provable rather than
/// inferred (CLO-554 AC2): a test that queues two responses and asserts the
/// counter reads 1 has shown the declined round issued no provider call.
///
/// The accept budget restarts for each queued response, so a slow multi-round
/// run is not cut off by time the earlier rounds consumed. When fewer requests
/// arrive than responses were queued, the pending waits time out in turn and
/// the thread exits, so `join()` always returns.
fn mock_ollama_server_multiple(
    responses: Vec<String>,
) -> (String, thread::JoinHandle<()>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&requests);
    let handle = thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        for body in responses {
            // Per-response budget: an earlier round's latency must not eat the
            // window a later one waits in.
            let start = std::time::Instant::now();
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        counter.fetch_add(1, Ordering::SeqCst);
                        // See mock_ollama_server: the accepted socket inherits
                        // O_NONBLOCK from the non-blocking listener on BSD/macOS.
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
                        let mut buf = [0u8; 4096];
                        let _ = stream.read(&mut buf);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        break;
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if start.elapsed() > std::time::Duration::from_secs(10) {
                            // Nobody is coming for this response; stop waiting
                            // for the rest either.
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => return,
                }
            }
        }
    });
    (format!("http://127.0.0.1:{port}"), handle, requests)
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

    // --no-finish: this test targets resolution + write-back, not the signed
    // finishing commit (which needs a signing key the CI runners lack).
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--yes", "--no-finish", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let content = fs::read_to_string(repo.join("f.txt")).unwrap();
    assert_eq!(content, "resolved\n");
    // The transaction stages what it applied; the merge is left for the user.
    let staged = git_str(repo, &["diff", "--cached", "--name-only"]);
    assert!(staged.lines().any(|l| l == "f.txt"), "staged: {staged}");
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

#[test]
fn resolve_validation_retry_then_escalate() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    // Serve two consecutive bad responses containing conflict markers
    let bad1 = mock_resolve_response("<<<<<<< HEAD\nstill conflict\n=======\n>>>>>>> feature\n");
    let bad2 =
        mock_resolve_response("<<<<<<< HEAD\nyet again conflict\n=======\n>>>>>>> feature\n");
    let (url, server, _requests) = mock_ollama_server_multiple(vec![bad1, bad2]);

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
    // CLO-555 / AC5: a retry that still fails ESCALATES the file (Partial,
    // markers kept, exit 0) instead of aborting the run.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "partial", "{stdout}");
    assert_eq!(json["files"][0]["action"], "escalated", "{stdout}");
    assert!(stderr.contains("escalating this file"), "stderr: {stderr}");
    assert!(
        fs::read_to_string(repo.join("f.txt"))
            .unwrap()
            .contains("<<<<<<<"),
        "escalated file keeps its markers"
    );
    assert!(merge_head_present(repo), "merge left in progress");
}

#[test]
fn resolve_validation_retry_success() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    // Serve a bad response first, then a successful clean response
    let bad = mock_resolve_response("<<<<<<< HEAD\nfirst bad try\n=======\n>>>>>>> feature\n");
    let clean = mock_resolve_response("clean and corrected resolution\n");
    let (url, server, _requests) = mock_ollama_server_multiple(vec![bad, clean]);

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    // --no-finish: retry behavior is the target, not the signed finish.
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--yes", "--no-finish", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = fs::read_to_string(repo.join("f.txt")).unwrap();
    assert_eq!(after, "clean and corrected resolution\n");
}

#[test]
fn provider_error_escalates_file_and_reports_partial() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    // Unreachable provider: the file escalates (owner decision 1), the run
    // reports Partial and exits 0, and the actionable error reaches stderr.
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", "http://127.0.0.1:1")],
        &["resolve", "--json", "--yes", "--provider", "ollama"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "partial", "{stdout}");
    assert_eq!(json["files"][0]["action"], "escalated", "{stdout}");
    assert!(
        stderr.contains("escalating this file"),
        "escalation explained: {stderr}"
    );
    assert!(
        stderr.contains("Ollama"),
        "actionable provider error surfaced: {stderr}"
    );
    assert!(merge_head_present(repo), "merge left in progress");
    assert!(
        fs::read_to_string(repo.join("f.txt"))
            .unwrap()
            .contains("<<<<<<<"),
        "escalated file keeps its markers"
    );
}

#[test]
fn early_failure_before_any_mutation_leaves_tree_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);
    let before = fs::read(repo.join("f.txt")).unwrap();

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    // No --yes on a closed stdin -> NonInteractive. The guard must fire
    // BEFORE the snapshot/zdiff3 re-checkout, so the merge-style markers are
    // byte-identical afterwards (no zdiff3 rewrite, no base section).
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", "http://127.0.0.1:1")],
        &["resolve", "--provider", "ollama"],
    );
    assert!(!out.status.success(), "NonInteractive is still an error");
    assert_eq!(
        fs::read(repo.join("f.txt")).unwrap(),
        before,
        "early exit must not rewrite conflicted files"
    );
}

// ---------------------------------------------------------------------------
// CLO-555 ownership transaction: stage, finish, escalation, --no-finish.
// Interactive paths (rejection restore, EOF at a prompt) need a real TTY and
// live in scripts/acceptance.sh (expect-driven), mirroring the AC-2 pattern;
// the snapshot/guard byte-level logic is unit-tested in src/resolve/mod.rs.
// ---------------------------------------------------------------------------

const OLLAMA_CONFIG: &str = r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#;

/// Probe whether `git commit -S` works here (mirrors scripts/acceptance.sh
/// `probe_signing`). CI runners have no signing key; those tests skip there.
fn signing_available() -> bool {
    let dir = tempfile::tempdir().unwrap();
    git_init(dir.path());
    run_git(
        dir.path(),
        &["commit", "-S", "--allow-empty", "-q", "-m", "probe"],
    )
    .status
    .success()
}

fn merge_head_present(repo: &Path) -> bool {
    run_git(repo, &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"])
        .status
        .success()
}

fn unmerged_paths(repo: &Path) -> String {
    git_str(repo, &["ls-files", "-u"])
}

fn staged_paths(repo: &Path) -> String {
    git_str(repo, &["diff", "--cached", "--name-only"])
}

#[test]
fn transaction_yes_merge_finishes_signed() {
    if !signing_available() {
        eprintln!("skipping transaction_yes_merge_finishes_signed: signing unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--json", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Single JSON envelope with the transaction fields.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "resolved", "{stdout}");
    assert_eq!(json["staged"][0], "f.txt", "{stdout}");
    assert_eq!(json["finish"]["result"], "completed", "{stdout}");
    assert_eq!(json["finish"]["op"], "merge", "{stdout}");
    assert!(
        json["finish"]["commit"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "{stdout}"
    );

    // Zero manual steps left: index clean, merge concluded by a signed
    // two-parent commit.
    assert!(!merge_head_present(repo), "MERGE_HEAD cleared");
    assert_eq!(unmerged_paths(repo), "", "no unmerged entries");
    assert!(
        run_git(repo, &["rev-parse", "--verify", "HEAD^2"])
            .status
            .success(),
        "HEAD is a 2-parent merge commit"
    );
    let raw = git_str(repo, &["cat-file", "commit", "HEAD"]);
    assert!(raw.contains("gpgsig"), "merge commit is signed: {raw}");
    assert_eq!(
        fs::read_to_string(repo.join("f.txt")).unwrap(),
        "resolved\n"
    );
}

#[test]
fn marker_free_unmerged_file_is_staged_without_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);
    // The user already resolved the file by hand before running gcm.
    fs::write(repo.join("f.txt"), "hand-resolved before gcm\n").unwrap();

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    // No provider call happens (no markers to resolve), so the base URL is a
    // dead address on purpose.
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", "http://127.0.0.1:1")],
        &[
            "resolve",
            "--json",
            "--yes",
            "--no-finish",
            "--provider",
            "ollama",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "resolved", "{stdout}");
    assert_eq!(json["files"][0]["action"], "accepted", "{stdout}");
    assert_eq!(json["staged"][0], "f.txt", "{stdout}");
    assert!(
        staged_paths(repo).lines().any(|l| l == "f.txt"),
        "hand-resolved file staged in the apply phase"
    );
    assert_eq!(
        unmerged_paths(repo),
        "",
        "unmerged entry cleared by staging"
    );
    assert_eq!(
        fs::read_to_string(repo.join("f.txt")).unwrap(),
        "hand-resolved before gcm\n",
        "content untouched"
    );
}

#[test]
fn escalation_stages_confirmed_progress_without_finishing() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    // Binary + text conflict: the text file resolves, the binary escalates.
    fs::write(repo.join(".gitattributes"), "*.bin binary\n").unwrap();
    fs::write(repo.join("img.bin"), b"\x89PNG\r\n\x1a\nBASE").unwrap();
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
    let _ = run_git(repo, &["merge", "feature"]);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    // No --no-finish: the escalation itself must prevent the finish, on any
    // machine (nothing signing-dependent runs).
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--json", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "partial", "{stdout}");
    assert_eq!(
        json["staged"][0], "f.txt",
        "confirmed work staged: {stdout}"
    );
    assert_eq!(json["finish"]["result"], "skipped", "{stdout}");
    assert!(
        staged_paths(repo).lines().any(|l| l == "f.txt"),
        "resolved file staged"
    );
    assert!(
        unmerged_paths(repo).contains("img.bin"),
        "escalated binary stays unmerged"
    );
    assert!(merge_head_present(repo), "merge left in progress");
}

#[test]
fn no_finish_reports_skipped_with_op() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &[
            "resolve",
            "--json",
            "--yes",
            "--no-finish",
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
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "resolved", "{stdout}");
    assert_eq!(json["finish"]["result"], "skipped", "{stdout}");
    assert_eq!(json["finish"]["op"], "merge", "{stdout}");
    assert!(merge_head_present(repo), "merge deliberately left open");
}

#[test]
fn finish_hook_failure_keeps_staged_and_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);
    // Rejecting pre-commit hook: fires before signing, so this runs on
    // unsigned machines and CI alike.
    let hooks = repo.join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(!out.status.success(), "finish failure must exit non-zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("could not finish the merge"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("git merge --continue"),
        "manual command named: {stderr}"
    );
    assert!(
        staged_paths(repo).lines().any(|l| l == "f.txt"),
        "staged resolution kept"
    );
    assert!(merge_head_present(repo), "MERGE_HEAD preserved for retry");
}

#[test]
fn cherry_pick_transaction_completes() {
    if !signing_available() {
        eprintln!("skipping cherry_pick_transaction_completes: signing unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
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
    let _ = run_git(repo, &["cherry-pick", "feature"]);

    let (url, server) = mock_ollama_server(&mock_resolve_response("resolved\n"));
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

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
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("cherry-pick completed"),
        "headline: {stdout}"
    );
    assert!(
        !run_git(
            repo,
            &["rev-parse", "--verify", "--quiet", "CHERRY_PICK_HEAD"]
        )
        .status
        .success(),
        "CHERRY_PICK_HEAD cleared"
    );
    assert_eq!(unmerged_paths(repo), "");
}

/// A 3-commit feature branch rebased onto a moved mainline, where BOTH feature
/// commits conflict on the same file. Stops on c1, then on c2.
///
/// Fixture invariant the round-counting tests depend on: exactly one conflicted
/// file with one Complex hunk per conflicting commit, so one queued mock
/// response serves exactly one round. Changing the file set or making a hunk
/// trivially auto-resolvable would silently change what the request counter
/// proves.
fn two_conflict_rebase(repo: &Path) -> String {
    git_init(repo);
    fs::write(repo.join("f.txt"), "base\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);
    let base = git_str(repo, &["branch", "--show-current"]);
    git(repo, &["switch", "-q", "-c", "feature"]);
    fs::write(repo.join("f.txt"), "f1\n").unwrap();
    git(repo, &["commit", "-qam", "c1"]);
    fs::write(repo.join("f.txt"), "f2\n").unwrap();
    git(repo, &["commit", "-qam", "c2"]);
    git(repo, &["switch", "-q", &base]);
    fs::write(repo.join("f.txt"), "moved\n").unwrap();
    git(repo, &["commit", "-qam", "move main"]);
    git(repo, &["switch", "-q", "feature"]);
    let _ = run_git(repo, &["rebase", &base]); // stops on c1
    base
}

/// A rebase is in progress iff git's sequencer directory exists. Do NOT test
/// `REBASE_HEAD` here: git leaves that ref behind after the rebase finishes, so
/// it reports a completed rebase as still running (the same trap the
/// production `Repo::is_rebasing` had).
fn rebase_in_progress(repo: &Path) -> bool {
    let out = run_git(repo, &["rev-parse", "--git-path", "rebase-merge"]);
    let merge_dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let out = run_git(repo, &["rev-parse", "--git-path", "rebase-apply"]);
    let apply_dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    repo.join(merge_dir).exists() || repo.join(apply_dir).exists()
}

#[test]
fn rebase_stops_on_next_conflict_reports_rerun() {
    if !signing_available() {
        eprintln!("skipping rebase_stops_on_next_conflict: signing unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    two_conflict_rebase(repo);

    // Two responses are queued but the cap allows one round: the counter then
    // proves the loop spent nothing past its own boundary (AC2's guarantee,
    // exercised here on the cap rather than the interactive gate - the gate
    // needs a TTY and lives in scripts/acceptance.sh AC-R3).
    let (url, server, requests) = mock_ollama_server_multiple(vec![
        mock_resolve_response("r1\n"),
        mock_resolve_response("r2\n"),
    ]);
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    // --max-rounds 1 is the deliberate opt-in to pre-CLO-554 behavior: one
    // provider call resolves the c1 stop, the finish continues the rebase,
    // which halts again applying c2, and the run ends there.
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &[
            "resolve",
            "--yes",
            "--max-rounds",
            "1",
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
    assert_eq!(
        requests.load(Ordering::SeqCst),
        1,
        "the capped-off round must not have issued a provider request"
    );

    // Legacy half of AC4: identical headline and git state.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("stopped on the next conflicted commit"),
        "headline: {stdout}"
    );
    assert!(rebase_in_progress(repo), "rebase still in progress");
    assert!(
        !unmerged_paths(repo).is_empty(),
        "next commit's conflict present"
    );
}

#[test]
fn max_rounds_one_emits_cap_reached_metadata() {
    // The other half of AC4: passing --max-rounds is an explicit opt-in, so the
    // run additionally reports why it stopped. (Split from the legacy pin above
    // because --json suppresses the human headline that test asserts.)
    if !signing_available() {
        eprintln!("skipping max_rounds_one_emits_cap_reached_metadata: signing unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    two_conflict_rebase(repo);

    let (url, server, requests) = mock_ollama_server_multiple(vec![
        mock_resolve_response("r1\n"),
        mock_resolve_response("r2\n"),
    ]);
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &[
            "resolve",
            "--json",
            "--yes",
            "--max-rounds",
            "1",
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
    assert_eq!(requests.load(Ordering::SeqCst), 1);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json envelope");
    assert_eq!(v["loop"]["terminal"], "cap_reached", "{stdout}");
    assert_eq!(v["loop"]["rounds_run"], 1, "{stdout}");
    assert_eq!(v["loop"]["max_rounds"], 1, "{stdout}");
    // The stop the user would see in `git status` is named for machines too.
    assert!(v["loop"]["stopped_on"].is_string(), "{stdout}");
}

#[test]
fn rebase_loop_completes_multi_commit_in_one_invocation() {
    if !signing_available() {
        eprintln!("skipping rebase_loop_completes_multi_commit: signing unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    two_conflict_rebase(repo);

    let (url, server, requests) = mock_ollama_server_multiple(vec![
        mock_resolve_response("r1\n"),
        mock_resolve_response("r2\n"),
    ]);
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--json", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2, "one call per round");

    // AC1: the whole rebase finished in this one invocation.
    assert!(!rebase_in_progress(repo), "rebase completed");
    assert_eq!(unmerged_paths(repo), "", "nothing left conflicted");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json envelope");
    assert_eq!(v["loop"]["terminal"], "completed", "{stdout}");
    assert_eq!(v["loop"]["rounds_run"], 2, "{stdout}");
    assert_eq!(v["loop"]["rounds"].as_array().unwrap().len(), 2, "{stdout}");
    // Each round names the commit it was applying.
    assert!(v["loop"]["rounds"][0]["commit"].is_string(), "{stdout}");
    assert!(v["loop"]["rounds"][1]["commit"].is_string(), "{stdout}");
    assert_ne!(
        v["loop"]["rounds"][0]["commit"], v["loop"]["rounds"][1]["commit"],
        "rounds must name different commits: {stdout}"
    );
    // Both feature commits landed on top of the moved mainline.
    let log = git_str(repo, &["log", "--oneline"]);
    assert!(log.contains("c1") && log.contains("c2"), "log: {log}");
}

#[test]
fn rebase_loop_escalation_stops_loop_and_keeps_earlier_rounds() {
    if !signing_available() {
        eprintln!("skipping rebase_loop_escalation: signing unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    two_conflict_rebase(repo);

    // Round 1 resolves; round 2's provider response is unusable, so that file
    // escalates. The finish is skipped, the report is Partial, and the loop
    // must not start a round 3.
    let (url, server, requests) = mock_ollama_server_multiple(vec![
        mock_resolve_response("r1\n"),
        "{\"message\":{\"content\":\"not json\"}}".to_string(),
        mock_resolve_response("never\n"),
    ]);
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--json", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "escalation is exit 0: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json envelope");
    assert_eq!(v["status"], "partial", "{stdout}");
    assert_eq!(v["loop"]["terminal"], "partial", "{stdout}");
    assert_eq!(v["loop"]["rounds_run"], 2, "no round 3: {stdout}");
    // Round 1's commit survives the round-2 escalation.
    let log = git_str(repo, &["log", "--oneline"]);
    assert!(log.contains("c1"), "round 1 stays committed: {log}");
    assert!(rebase_in_progress(repo), "rebase left stopped");
    // A third round was never attempted, so the third response went unused.
    assert!(
        requests.load(Ordering::SeqCst) <= 2,
        "loop must not spend after an escalation"
    );
}

#[test]
fn cherry_pick_sequence_loops_on_the_same_signal() {
    if !signing_available() {
        eprintln!("skipping cherry_pick_sequence_loops: signing unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    fs::write(repo.join("f.txt"), "base\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);
    let base = git_str(repo, &["branch", "--show-current"]);
    git(repo, &["switch", "-q", "-c", "feature"]);
    fs::write(repo.join("f.txt"), "f1\n").unwrap();
    git(repo, &["commit", "-qam", "c1"]);
    fs::write(repo.join("f.txt"), "f2\n").unwrap();
    git(repo, &["commit", "-qam", "c2"]);
    git(repo, &["switch", "-q", &base]);
    fs::write(repo.join("f.txt"), "moved\n").unwrap();
    git(repo, &["commit", "-qam", "move main"]);
    // Cherry-picking a RANGE sequences the same way a rebase does.
    let _ = run_git(repo, &["cherry-pick", &format!("{base}..feature")]);

    let (url, server, _requests) = mock_ollama_server_multiple(vec![
        mock_resolve_response("p1\n"),
        mock_resolve_response("p2\n"),
    ]);
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--json", "--yes", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json envelope");
    // Keying on StoppedOnConflict rather than the op name is what makes this
    // work with no cherry-pick-specific code.
    assert_eq!(
        v["loop"]["rounds"][0]["finish"]["op"], "cherry-pick",
        "{stdout}"
    );
    assert!(
        v["loop"]["rounds_run"].as_u64().unwrap() >= 2,
        "the sequence looped: {stdout}"
    );
}

#[test]
fn max_rounds_zero_is_rejected_at_parse_time() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);
    let before = fs::read(repo.join("f.txt")).unwrap();
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", "http://127.0.0.1:1")],
        &[
            "resolve",
            "--yes",
            "--max-rounds",
            "0",
            "--provider",
            "ollama",
        ],
    );
    assert!(!out.status.success(), "0 rounds is not a runnable request");
    // clap rejects before main ever runs, so the tree is untouched.
    assert_eq!(
        fs::read(repo.join("f.txt")).unwrap(),
        before,
        "no mutation on a rejected flag"
    );
}

#[test]
fn config_max_rounds_zero_is_rejected_before_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);
    let before = fs::read(repo.join("f.txt")).unwrap();
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(
        cfg_dir.path(),
        "version = 2\ndefault = \"ollama\"\n\n[[providers]]\nid = \"ollama\"\n\n[conflict]\nmax_rounds = 0\n",
    );

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", "http://127.0.0.1:1")],
        &["resolve", "--yes", "--provider", "ollama"],
    );
    assert!(!out.status.success(), "a configured 0 is rejected too");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("max_rounds must be at least 1"),
        "actionable message: {stderr}"
    );
    // Rejected before the snapshot/zdiff3 re-checkout: markers untouched.
    assert_eq!(
        fs::read(repo.join("f.txt")).unwrap(),
        before,
        "no mutation on a rejected config"
    );
}

#[test]
fn max_rounds_flag_beats_config() {
    if !signing_available() {
        eprintln!("skipping max_rounds_flag_beats_config: signing unavailable");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    two_conflict_rebase(repo);

    let (url, server, requests) = mock_ollama_server_multiple(vec![
        mock_resolve_response("r1\n"),
        mock_resolve_response("r2\n"),
    ]);
    let cfg_dir = tempfile::tempdir().unwrap();
    // Config says 5; the flag says 1. The flag wins, so the loop stops after
    // one round and the second queued response is never requested.
    write_config(
        cfg_dir.path(),
        "version = 2\ndefault = \"ollama\"\n\n[[providers]]\nid = \"ollama\"\n\n[conflict]\nmax_rounds = 5\n",
    );

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &[
            "resolve",
            "--json",
            "--yes",
            "--max-rounds",
            "1",
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
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json envelope");
    assert_eq!(v["loop"]["max_rounds"], 1, "CLI overrides config: {stdout}");
    assert_eq!(v["loop"]["terminal"], "cap_reached", "{stdout}");
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[test]
fn no_finish_runs_a_single_round_without_loop_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    two_conflict_rebase(repo);

    let (url, server, requests) = mock_ollama_server_multiple(vec![
        mock_resolve_response("r1\n"),
        mock_resolve_response("r2\n"),
    ]);
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    // --no-finish never continues the rebase, so no second stop can arise.
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &[
            "resolve",
            "--json",
            "--yes",
            "--no-finish",
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
    assert_eq!(requests.load(Ordering::SeqCst), 1, "exactly one round");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("\"loop\""),
        "a single-round mode emits the pre-CLO-554 envelope: {stdout}"
    );
}

#[test]
fn dry_run_runs_a_single_round_without_loop_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    two_conflict_rebase(repo);

    let (url, server, _requests) = mock_ollama_server_multiple(vec![mock_resolve_response("r1\n")]);
    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("GCM_OLLAMA_BASE_URL", &url)],
        &["resolve", "--json", "--dry-run", "--provider", "ollama"],
    );
    server.join().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("\"loop\""),
        "dry-run previews the first stop only: {stdout}"
    );
    assert!(rebase_in_progress(repo), "dry-run changed nothing");
}

#[test]
fn mergiraf_resolution_is_a_proposal_and_gets_staged() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    // Fake mergiraf on PATH: fully "resolves" the file structurally.
    let bin = dir.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("mergiraf");
    fs::write(
        &fake,
        "#!/bin/sh\n[ \"$1\" = solve ] || exit 1\nprintf 'mergiraf resolved\\n' > \"$4\"\nexit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path_env = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), OLLAMA_CONFIG);

    // No provider call: mergiraf resolves everything, and its output flows
    // through the same confirm (auto-accepted by --yes) + stage pipeline.
    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[
            ("GCM_OLLAMA_BASE_URL", "http://127.0.0.1:1"),
            ("PATH", &path_env),
        ],
        &[
            "resolve",
            "--json",
            "--yes",
            "--no-finish",
            "--provider",
            "ollama",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(json["status"], "resolved", "{stdout}");
    assert_eq!(json["files"][0]["action"], "accepted", "{stdout}");
    assert_eq!(json["staged"][0], "f.txt", "{stdout}");
    assert_eq!(
        fs::read_to_string(repo.join("f.txt")).unwrap(),
        "mergiraf resolved\n"
    );
    assert!(staged_paths(repo).lines().any(|l| l == "f.txt"));
}
