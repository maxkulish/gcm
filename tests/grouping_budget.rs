//! Grouping prompt budget integration tests (CLO-797).
//!
//! Each test drives the built `gcm` binary against a throwaway repo and a local
//! capture listener on `127.0.0.1:0`, wired in through the provider's base-URL
//! override. The listener records every request body in full and answers each
//! one from a script, so a test can assert both what gcm sent and that it sent
//! nothing at all. No network access and no API key are needed.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const PROVIDER_ENV: &[&str] = &[
    "GROQ_API_KEY",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GCM_PROVIDER",
    "GCM_MODEL",
    "GCM_LOG_LEVEL",
    "GCM_DEBUG",
    "OLLAMA_HOST",
    "GCM_OLLAMA_BASE_URL",
    "GCM_OPENAI_BASE_URL",
    "GCM_GROQ_BASE_URL",
    "GCM_ANTHROPIC_BASE_URL",
    "GCM_CONTEXT_WINDOW_TOKENS",
    "GCM_DIFF_TOTAL_BYTES",
    "GCM_DIFF_PER_FILE_BYTES",
];

/// gpt-oss-120b's window on Groq, from the investigation's 400.
const GROQ_WINDOW: usize = 131_072;

/// The requests a capture listener received, in arrival order.
type Captured = Arc<Mutex<Vec<String>>>;

/// Serve `replies` in order, one per request, recording each request body.
/// Requests past the end of the script get the last reply.
fn capture(replies: Vec<(u16, String)>) -> (String, Captured) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let captured: Captured = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&captured);
    thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        let mut served = 0usize;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(60) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                    let body = read_request(&stream);
                    sink.lock().unwrap().push(body);
                    let (status, reply) = &replies[served.min(replies.len() - 1)];
                    served += 1;
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply.len(),
                        reply
                    );
                    let mut stream = stream;
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return,
            }
        }
    });
    (format!("http://127.0.0.1:{port}"), captured)
}

/// Read one HTTP request and return its body, honouring `Content-Length`.
fn read_request(stream: &std::net::TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    String::from_utf8_lossy(&body).into_owned()
}

/// A Groq-style 400 for every call, so a run stops after its requests.
fn bad_request() -> (u16, String) {
    (
        400,
        serde_json::json!({"error": {"message": "captured", "type": "invalid_request_error"}})
            .to_string(),
    )
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("run git")
        .success();
    assert!(ok, "git {args:?} failed");
}

fn git_init(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "Test"]);
}

/// The investigation's shape: 515 untracked files under long paths, each with
/// a few KiB of content, so the read caps fire and the path list dominates.
fn large_tree(dir: &Path) -> Vec<String> {
    git_init(dir);
    let base = "frontend/packages/design-system/src/components/navigation/sidebar/sections";
    let mut paths = Vec::new();
    for i in 0..515 {
        let rel = format!(
            "{base}/group_{:02}/collapsible_section_item_{i:04}.tsx",
            i % 20
        );
        let full = dir.join(&rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        let content: String = (0..120)
            .map(|line| format!("export const value_{i}_{line} = {line};\n"))
            .collect();
        std::fs::write(full, content).unwrap();
        paths.push(rel);
    }
    paths
}

fn command(repo: &Path, cfg: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(args)
        .env("GCM_CONFIG", cfg)
        .env("GCM_RETRY_MAX", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for var in PROVIDER_ENV {
        cmd.env_remove(var);
    }
    cmd
}

fn groq(repo: &Path, cfg: &Path, url: &str, args: &[&str]) -> Command {
    let mut cmd = command(repo, cfg, args);
    cmd.env("GROQ_API_KEY", "sk-test")
        .env("GCM_GROQ_BASE_URL", url);
    cmd
}

fn run(mut cmd: Command) -> Output {
    cmd.output().expect("run gcm")
}

/// gcm's own estimator, applied to a whole request body: 2 bytes per token.
fn estimated_tokens(payload: &str) -> usize {
    payload.len().div_ceil(2)
}

/// AC1: the 515-file tree no longer produces a grouping request larger than
/// the model's window. This is the regression guard: against the pre-fix
/// assembly (file list + status + per-file stubs) it fails.
#[test]
fn ac1_grouping_payload_fits_the_window() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    large_tree(repo.path());
    let (url, captured) = capture(vec![bad_request()]);

    let out = run(groq(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"]));
    let requests = captured.lock().unwrap().clone();
    assert!(
        !requests.is_empty(),
        "no request reached the listener: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let plan: serde_json::Value = serde_json::from_str(&requests[0]).expect("JSON payload");
    assert!(
        plan.get("response_format").is_some(),
        "first request is the grouping call"
    );
    let tokens = estimated_tokens(&requests[0]);
    assert!(
        tokens < GROQ_WINDOW,
        "grouping payload is ~{tokens} estimated tokens, over the {GROQ_WINDOW}-token window"
    );
}
