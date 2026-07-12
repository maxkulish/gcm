//! End-to-end acceptance test for the Vertex AI provider (CLO-537).
//!
//! Drives the built `gcm` binary against a local mock `generateContent` server via
//! `GCM_VERTEX_BASE_URL` + `GCM_VERTEX_TOKEN`, so the full Vertex HTTP path is
//! exercised hermetically (no gcloud, no network): `request()` builds the Vertex URL,
//! sends the Bearer token via `extra_headers` with `auth: None`, `post_json` performs
//! the round-trip, and `gemini::extract_text` parses the Gemini-shaped response. The
//! live variant (a real 200 against a GCP project) is the HITL step, out of scope here.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;

/// Env vars scrubbed before each run so a developer's real Vertex/GCP config can't
/// leak into the hermetic test.
const SCRUB_ENV: &[&str] = &[
    "GROQ_API_KEY",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GCM_PROVIDER",
    "GCM_VERTEX_PROJECT",
    "GCM_VERTEX_LOCATION",
    "GCM_VERTEX_TOKEN",
    "GCM_VERTEX_BASE_URL",
    "GOOGLE_CLOUD_PROJECT",
    "GCP_PROJECT",
    "GOOGLE_CLOUD_LOCATION",
    "GCP_REGION",
];

fn git_init(dir: &Path) {
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "t@example.com"],
        vec!["config", "user.name", "Test"],
    ] {
        assert!(
            Command::new("git")
                .args(&args)
                .current_dir(dir)
                .status()
                .expect("run git")
                .success(),
            "git {args:?} failed"
        );
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
    for var in SCRUB_ENV {
        cmd.env_remove(var);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.output().expect("run gcm")
}

/// A Gemini-shaped `generateContent` response whose single text part carries the
/// resolve JSON (`{"resolutions":[{hunk_index, replacement}]}`), matching what
/// `gemini::extract_text` (reused by Vertex) expects.
fn mock_vertex_resolve_response(replacement: &str) -> String {
    let inner = serde_json::json!({
        "resolutions": [{ "hunk_index": 0, "replacement": replacement }]
    })
    .to_string();
    serde_json::json!({
        "candidates": [{
            "content": { "parts": [{ "text": inner }] },
            "finishReason": "STOP"
        }]
    })
    .to_string()
}

/// Serve `body` once on a random port, capturing the request line/headers so the test
/// can assert the Vertex URL + Bearer header were actually sent. Returns
/// `(base_url, handle)`; `handle.join()` yields the captured request bytes.
fn mock_server(body: String) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        let start = std::time::Instant::now();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
                    let mut buf = [0u8; 8192];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    return req;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() > std::time::Duration::from_secs(10) {
                        return String::new();
                    }
                    thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => return String::new(),
            }
        }
    });
    (format!("http://127.0.0.1:{port}"), handle)
}

fn git(repo: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .expect("run git")
            .status
            .success(),
        "git {args:?} failed"
    );
}

/// A real 3-way conflict on `f.txt` (base/feature/mainline all differ), so the
/// resolver classifies it Complex and calls the provider.
fn create_conflict(repo: &Path) {
    fs::write(repo.join("f.txt"), "base\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);
    let base = String::from_utf8_lossy(
        &Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(repo)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    git(repo, &["switch", "-q", "-c", "feature"]);
    fs::write(repo.join("f.txt"), "feature\n").unwrap();
    git(repo, &["commit", "-qam", "feature"]);
    git(repo, &["switch", "-q", &base]);
    fs::write(repo.join("f.txt"), "mainline\n").unwrap();
    git(repo, &["commit", "-qam", "mainline"]);
    // Expected to fail (leaves conflict markers) - that's the state resolve acts on.
    let _ = Command::new("git")
        .args(["merge", "feature"])
        .current_dir(repo)
        .output();
}

const VERTEX_CONFIG: &str = r#"version = 2
default = "vertex"

[[providers]]
id = "vertex"
project = "test-proj"
"#;

#[test]
fn resolve_via_vertex_hits_mock_generatecontent_with_bearer_and_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    let (url, server) = mock_server(mock_vertex_resolve_response("RESOLVED_BY_VERTEX\n"));

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), VERTEX_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[
            ("GCM_VERTEX_BASE_URL", &url),
            ("GCM_VERTEX_TOKEN", "fake-adc-token"),
        ],
        &["resolve", "--yes", "--no-finish", "--provider", "vertex"],
    );
    let request = server.join().unwrap();

    assert!(
        out.status.success(),
        "gcm resolve --provider vertex failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // The Vertex request actually reached the server with the right URL shape + auth.
    assert!(
        request.contains("/v1/projects/test-proj/locations/global/publishers/google/models/"),
        "request line missing Vertex URL path: {request}"
    );
    assert!(
        request.contains(":generateContent"),
        "request missing :generateContent: {request}"
    );
    assert!(
        request
            .to_lowercase()
            .contains("authorization: bearer fake-adc-token"),
        "request missing Bearer token header: {request}"
    );

    // The provider's resolution was applied: markers gone, replacement present.
    let after = fs::read_to_string(repo.join("f.txt")).unwrap();
    assert!(
        !after.contains("<<<<<<<") && !after.contains(">>>>>>>"),
        "conflict markers should be gone after resolve: {after:?}"
    );
    assert!(
        after.contains("RESOLVED_BY_VERTEX"),
        "resolved content should come from the mock Vertex response: {after:?}"
    );
}

/// Resolve a tool's absolute path from the current PATH (via `command -v`).
#[cfg(unix)]
fn tool_path(name: &str) -> String {
    let out = Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .expect("run sh");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(unix)]
#[test]
fn vertex_missing_gcloud_and_token_is_actionable_not_a_panic() {
    // AC-3: no GCM_VERTEX_TOKEN and gcloud absent -> an actionable, typed error (no
    // panic, no "check <env_var>"). Build a clean bin dir that has git + sh symlinked
    // (so gcm's own git work still runs) but NOT gcloud, then point PATH there.
    use std::os::unix::fs::symlink;

    let git = tool_path("git");
    let sh = tool_path("sh");
    if git.is_empty() || sh.is_empty() {
        return; // environment without a resolvable git/sh; skip (never on CI).
    }

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git_init(repo);
    create_conflict(repo);

    let bin = tempfile::tempdir().unwrap();
    symlink(&git, bin.path().join("git")).unwrap();
    symlink(&sh, bin.path().join("sh")).unwrap();

    let cfg_dir = tempfile::tempdir().unwrap();
    write_config(cfg_dir.path(), VERTEX_CONFIG);

    let out = run_gcm(
        repo,
        cfg_dir.path(),
        &[("PATH", &bin.path().to_string_lossy())],
        &["resolve", "--yes", "--provider", "vertex"],
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "expected failure without a token: {combined}"
    );
    assert!(!combined.contains("panicked"), "must not panic: {combined}");
    assert!(
        combined.contains("gcloud") || combined.contains("GCM_VERTEX_TOKEN"),
        "error should mention gcloud/ADC or the token env var: {combined}"
    );
}
