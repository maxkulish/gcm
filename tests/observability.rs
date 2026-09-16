//! Provider-call visibility integration tests (CLO-798).
//!
//! Each test drives the built `gcm` binary as a subprocess against a throwaway
//! git repo, a private `GCM_CONFIG`, a cleared provider environment, and a
//! scripted `TcpListener` stub on `127.0.0.1:0` wired in through
//! `GCM_GROQ_BASE_URL`. Because the child's stderr is a pipe, every test also
//! exercises the non-TTY path: no `\r`, no ANSI escape (AC-8).
//!
//! The wizard's model-list budget (AC-4, evaluation row 8) is covered by the
//! library unit test `model_fetch_timeout_names_its_own_budget` instead: the
//! `gcm provider` wizard reads `/dev/tty` and cannot be driven from here, which
//! is the same reason `tests/provider.rs` verifies it by hand.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const PROVIDER_ENV: &[&str] = &[
    "GROQ_API_KEY",
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GCM_PROVIDER",
    "GCM_LOG_LEVEL",
    "GCM_DEBUG",
    "OLLAMA_HOST",
    "GCM_OLLAMA_BASE_URL",
    "GCM_OPENAI_BASE_URL",
    "GCM_GROQ_BASE_URL",
];

/// One scripted server response, consumed by one incoming request.
#[derive(Clone)]
enum Act {
    /// Wait, then answer with this status and body.
    Reply(Duration, u16, String),
    /// Accept and read the request, then never answer: the client hits its
    /// budget before any response header arrives (`ErrorKind::Timeout`).
    StallHeaders,
    /// Answer `200` with a `Content-Length` and then never send the body: the
    /// client stalls mid-read, which classifies as `Transport("timeout...")`.
    StallBody,
}

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

/// A repo with `n` unstaged files, so the grouping path has something to send.
fn repo_with_files(dir: &Path, n: usize) {
    git_init(dir);
    for i in 0..n {
        std::fs::write(dir.join(format!("f{i}.txt")), format!("line {i}\n")).unwrap();
    }
}

/// An OpenAI-compatible chat body whose content is a valid one-group plan.
fn plan_body(files: &[&str]) -> String {
    let plan = serde_json::json!({
        "groups": [{
            "files": files,
            "summary": "test group",
            "commit_message": "feat: test"
        }]
    })
    .to_string();
    serde_json::json!({ "choices": [{ "message": { "content": plan } }] }).to_string()
}

/// An OpenAI-compatible chat body carrying a single commit message.
fn message_body() -> String {
    serde_json::json!({ "choices": [{ "message": { "content": "feat: test" } }] }).to_string()
}

/// A Groq-shaped 400 whose signal lives in the sibling `error.code` and past
/// character 200 of `error.message` - the two extraction hazards from AC-5.
fn context_window_400() -> String {
    let padding = "the request could not be served for the following reason: ".repeat(4);
    serde_json::json!({
        "error": {
            "message": format!("{padding}please reduce the length of the messages"),
            "type": "invalid_request_error",
            "code": "context_length_exceeded"
        }
    })
    .to_string()
}

/// Serve `script` in order, one act per incoming request, and return the base
/// URL. The thread exits once the script is exhausted or nothing else arrives,
/// so it never outlives the test.
fn stub(script: Vec<Act>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        for act in script {
            let start = Instant::now();
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // The accepted socket inherits O_NONBLOCK from the
                        // listener on BSD/macOS, which would make the read
                        // timeout a no-op (see tests/resolve_integration.rs).
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                        let mut buf = [0u8; 8192];
                        let _ = stream.read(&mut buf);
                        match &act {
                            Act::Reply(delay, status, body) => {
                                thread::sleep(*delay);
                                let response = format!(
                                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                    body.len(),
                                    body
                                );
                                let _ = stream.write_all(response.as_bytes());
                            }
                            Act::StallHeaders => thread::sleep(Duration::from_secs(20)),
                            Act::StallBody => {
                                let _ = stream.write_all(
                                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 4096\r\n\r\n",
                                );
                                let _ = stream.flush();
                                thread::sleep(Duration::from_secs(20));
                            }
                        }
                        break;
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if start.elapsed() > Duration::from_secs(30) {
                            return;
                        }
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => return,
                }
            }
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn command(repo: &Path, cfg: &Path, base_url: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(args)
        .env("GCM_CONFIG", cfg)
        .env("GROQ_API_KEY", "sk-test")
        .env("GCM_GROQ_BASE_URL", base_url)
        .env("GCM_RETRY_MAX", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for var in PROVIDER_ENV {
        if *var != "GROQ_API_KEY" && *var != "GCM_GROQ_BASE_URL" {
            cmd.env_remove(var);
        }
    }
    cmd
}

fn run(repo: &Path, cfg: &Path, base_url: &str, args: &[&str]) -> Output {
    command(repo, cfg, base_url, args)
        .output()
        .expect("run gcm")
}

/// AC-8: whatever else a test asserts, a piped stderr must stay plain text.
fn assert_plain(stderr: &str) {
    assert!(!stderr.contains('\r'), "CR on a non-TTY stderr: {stderr:?}");
    assert!(
        !stderr.contains('\x1b'),
        "ANSI escape on a non-TTY stderr: {stderr:?}"
    );
}

/// AC-1: a stalled call has to prove it is alive. Reads the child's stderr line
/// by line, timestamping each, so both "something within 2s" and "never more
/// than 5s of silence" are measured rather than inferred.
#[test]
fn slow_endpoint_emits_progress() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 3);
    let url = stub(vec![Act::Reply(
        Duration::from_secs(12),
        200,
        plan_body(&["f0.txt", "f1.txt", "f2.txt"]),
    )]);

    let started = Instant::now();
    // Deliberately not `--json`: the ticker is the thing under test and AC-7
    // switches it off there.
    let mut child = command(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"])
        .env("GCM_HTTP_TIMEOUT_SECS", "30")
        .spawn()
        .expect("spawn gcm");

    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send((started.elapsed(), line)).is_err() {
                return;
            }
        }
    });

    let mut stamps: Vec<(Duration, String)> = Vec::new();
    while let Ok(entry) = rx.recv_timeout(Duration::from_secs(30)) {
        stamps.push(entry);
    }
    let _ = child.wait();

    assert!(!stamps.is_empty(), "the call produced no stderr at all");
    let transcript: Vec<&str> = stamps.iter().map(|(_, l)| l.as_str()).collect();
    assert!(
        stamps[0].0 < Duration::from_secs(2),
        "first sign of life after {:?}: {transcript:?}",
        stamps[0].0
    );

    // AC-1's bound exactly: the 4s interval leaves room for the render and the
    // scheduler without the observed gap crossing 5s.
    let mut previous = Duration::ZERO;
    for (at, line) in &stamps {
        assert!(
            *at - previous < Duration::from_secs(5),
            "{:?} of silence before {line:?}: {transcript:?}",
            *at - previous
        );
        previous = *at;
    }
    assert!(
        stamps.len() >= 3,
        "a 12s call should tick more than once: {transcript:?}"
    );
    assert_plain(&transcript.join("\n"));
    assert!(
        transcript.iter().any(|l| l.contains("still waiting")),
        "non-TTY progress is a whole line: {transcript:?}"
    );
}

/// AC-2: the pre-call line says what gcm is about to do and with what.
#[test]
fn status_line_names_operation() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 3);
    let url = stub(vec![Act::Reply(
        Duration::ZERO,
        200,
        plan_body(&["f0.txt", "f1.txt", "f2.txt"]),
    )]);

    let out = run(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr
        .lines()
        .find(|l| l.starts_with("gcm: grouping:"))
        .unwrap_or_else(|| panic!("no grouping status line: {stderr}"));
    assert!(line.contains("Groq"), "{line}");
    assert!(line.contains("3 file(s)"), "{line}");
    assert!(line.contains('~'), "prompt size is approximate: {line}");
    assert_plain(&stderr);
}

/// AC-3: a retry says so with no logging variable set at all.
#[test]
fn retry_notices_visible_by_default() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 1);
    let url = stub(vec![
        Act::Reply(Duration::ZERO, 429, "{}".to_string()),
        Act::Reply(Duration::ZERO, 429, "{}".to_string()),
        Act::Reply(Duration::ZERO, 200, plan_body(&["f0.txt"])),
    ]);

    let out = command(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"])
        .env("GCM_RETRY_MAX", "3")
        .env("GCM_RETRY_BASE_MS", "10")
        .output()
        .expect("run gcm");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "run failed: {stderr}");
    assert!(stderr.contains("attempt 1 of 4"), "{stderr}");
    assert!(stderr.contains("attempt 2 of 4"), "{stderr}");
    assert!(stderr.contains("rate limited"), "{stderr}");
    assert!(stderr.contains("retrying in"), "{stderr}");
    assert_plain(&stderr);
}

/// AC-4 phase A: no response headers within the budget.
#[test]
fn timeout_before_headers() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 1);
    let url = stub(vec![Act::StallHeaders, Act::StallHeaders]);

    let out = command(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"])
        .env("GCM_HTTP_TIMEOUT_SECS", "1")
        .output()
        .expect("run gcm");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("within 1s"),
        "names the real budget: {stderr}"
    );
    assert!(stderr.contains("GCM_HTTP_TIMEOUT_SECS"), "{stderr}");
    assert_plain(&stderr);
}

/// AC-4 phase B: headers arrive, the body never does. The transport calls this
/// `Transport("timeout...")`, not `Timeout`, and it must still be recognised.
#[test]
fn timeout_mid_body() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 1);
    let url = stub(vec![Act::StallBody, Act::StallBody]);

    let out = command(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"])
        .env("GCM_HTTP_TIMEOUT_SECS", "1")
        .output()
        .expect("run gcm");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("within 1s"), "{stderr}");
    assert!(stderr.contains("GCM_HTTP_TIMEOUT_SECS"), "{stderr}");
    assert_plain(&stderr);
}

/// AC-5: an oversized grouping prompt is the user's diff, not a gcm bug.
#[test]
fn context_window_message_grouping() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 4);
    let url = stub(vec![
        Act::Reply(Duration::ZERO, 400, context_window_400()),
        Act::Reply(Duration::ZERO, 400, context_window_400()),
    ]);

    let out = run(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("too large for this model"), "{stderr}");
    assert!(stderr.contains("4 file(s)"), "{stderr}");
    assert!(stderr.contains('~'), "names the prompt size: {stderr}");
    assert!(
        stderr.contains("--all"),
        "grouping can suggest --all: {stderr}"
    );
    assert!(!stderr.contains("gcm bug"), "{stderr}");
    assert!(!stderr.contains("please report it"), "{stderr}");
    assert_plain(&stderr);
}

/// AC-5: the advice has to differ per call site. On the `--all` path, telling
/// the user to retry with `--all` would be a loop.
#[test]
fn context_window_advice_is_operation_specific() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 4);
    let url = stub(vec![Act::Reply(Duration::ZERO, 400, context_window_400())]);

    let out = run(
        repo.path(),
        cfg.path(),
        &url,
        &["--all", "--dry-run", "--yes"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("single-commit message"), "{stderr}");
    assert!(stderr.contains("GCM_DIFF_TOTAL_BYTES"), "{stderr}");
    assert!(!stderr.contains("--all"), "circular advice: {stderr}");
    assert!(
        !stderr.to_lowercase().contains("stage"),
        "staging does not narrow the prompt: {stderr}"
    );
    assert!(!stderr.contains("gcm bug"), "{stderr}");
    assert_plain(&stderr);
}

/// AC-6: where the prompt bytes went, so a size regression is diagnosable.
#[test]
fn debug_logs_prompt_sections() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 2);
    let url = stub(vec![Act::Reply(
        Duration::ZERO,
        200,
        plan_body(&["f0.txt", "f1.txt"]),
    )]);

    let out = command(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"])
        .env("GCM_LOG_LEVEL", "debug")
        .output()
        .expect("run gcm");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr
        .lines()
        .find(|l| l.contains("grouping prompt sections:"))
        .unwrap_or_else(|| panic!("no prompt-size line: {stderr}"));
    for section in ["file_list=", "status=", "stat=", "body=", "total="] {
        assert!(line.contains(section), "missing {section}: {line}");
    }
}

/// AC-7: the frozen half of the `--json` contract. Progress is on stderr, so
/// stdout stays exactly one envelope, and `error.code` still derives from the
/// untouched `ErrorKind` even where the prose was replaced.
#[test]
fn json_contract_frozen() {
    /// The envelope's own keys, sorted. A new top-level key, or a lost one, is
    /// the shape change AC-7 freezes - the two prose fields live one level down.
    fn keys(env: &serde_json::Value) -> Vec<String> {
        let mut k: Vec<String> = env.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    }

    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 2);
    let url = stub(vec![Act::Reply(
        Duration::ZERO,
        200,
        plan_body(&["f0.txt", "f1.txt"]),
    )]);
    let out = run(
        repo.path(),
        cfg.path(),
        &url,
        &["--dry-run", "--yes", "--json"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim().lines().count(), 1, "one envelope: {stdout}");
    let env: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(env["v"], 1);
    assert_eq!(env["status"], "plan");
    assert_eq!(env["mode"], "dry_run");
    assert_eq!(
        keys(&env),
        [
            "cached",
            "changed_files",
            "mode",
            "model",
            "plan",
            "provider",
            "status",
            "v"
        ]
    );

    // A replaced error message must not move `error.code`.
    let repo2 = tempfile::tempdir().unwrap();
    let cfg2 = tempfile::tempdir().unwrap();
    repo_with_files(repo2.path(), 2);
    let url2 = stub(vec![
        Act::Reply(Duration::ZERO, 400, context_window_400()),
        Act::Reply(Duration::ZERO, 400, context_window_400()),
    ]);
    let out2 = run(
        repo2.path(),
        cfg2.path(),
        &url2,
        &["--dry-run", "--yes", "--json"],
    );
    let stdout2 = String::from_utf8_lossy(&out2.stdout);
    assert_eq!(stdout2.trim().lines().count(), 1, "one envelope: {stdout2}");
    let env2: serde_json::Value = serde_json::from_str(stdout2.trim()).unwrap();
    assert_eq!(env2["status"], "error");
    assert_eq!(env2["error"]["code"], "Provider");
    assert_eq!(
        keys(&env2),
        ["error", "mode", "model", "provider", "status", "v"]
    );
    let mut error_keys: Vec<&String> = env2["error"].as_object().unwrap().keys().collect();
    error_keys.sort();
    assert_eq!(error_keys, ["code", "message"]);
    assert!(
        !env2["error"]["message"]
            .as_str()
            .unwrap()
            .contains("gcm bug"),
        "{stdout2}"
    );

    // `noop`: no provider call at all, so nothing new can reach stdout.
    let repo3 = tempfile::tempdir().unwrap();
    let cfg3 = tempfile::tempdir().unwrap();
    git_init(repo3.path());
    let out3 = run(repo3.path(), cfg3.path(), "http://127.0.0.1:1", &["--json"]);
    let stdout3 = String::from_utf8_lossy(&out3.stdout);
    let env3: serde_json::Value = serde_json::from_str(stdout3.trim()).unwrap();
    assert_eq!(env3["v"], 1);
    assert_eq!(env3["status"], "noop", "{stdout3}");
    assert_eq!(keys(&env3), ["mode", "status", "v"], "{stdout3}");

    // `committed`: the happy path still emits exactly one envelope.
    let repo4 = tempfile::tempdir().unwrap();
    let cfg4 = tempfile::tempdir().unwrap();
    repo_with_files(repo4.path(), 2);
    let url4 = stub(vec![Act::Reply(
        Duration::ZERO,
        200,
        plan_body(&["f0.txt", "f1.txt"]),
    )]);
    let out4 = run(repo4.path(), cfg4.path(), &url4, &["--yes", "--json"]);
    let stdout4 = String::from_utf8_lossy(&out4.stdout);
    assert_eq!(stdout4.trim().lines().count(), 1, "one envelope: {stdout4}");
    let env4: serde_json::Value = serde_json::from_str(stdout4.trim()).unwrap();
    assert_eq!(env4["v"], 1);
    assert_eq!(env4["status"], "committed", "{stdout4}");
    assert_eq!(env4["mode"], "grouped", "{stdout4}");
    assert_eq!(
        keys(&env4),
        [
            "commit",
            "group_progress",
            "mode",
            "model",
            "provider",
            "status",
            "v"
        ],
        "{stdout4}"
    );
}

/// AC-11: a `--json` consumer sees two provider requests; it has to be told why
/// the second one happened.
#[test]
fn transition_announced_under_json() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 2);
    let url = stub(vec![
        Act::Reply(Duration::ZERO, 400, context_window_400()),
        Act::Reply(Duration::ZERO, 200, message_body()),
    ]);

    let out = run(repo.path(), cfg.path(), &url, &["--yes", "--json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stderr.contains("Falling back to single-commit mode."),
        "transition not announced under --json: {stderr}"
    );
    let env: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(env["status"], "fallback", "{stdout}");
    assert_eq!(env["fallback"]["raw_code"], "BadRequest", "{stdout}");
    let mut fallback_keys: Vec<&String> = env["fallback"].as_object().unwrap().keys().collect();
    fallback_keys.sort();
    assert_eq!(fallback_keys, ["commit", "raw_code", "reason"], "{stdout}");
    // Two calls, two different operation labels.
    assert!(stderr.contains("gcm: grouping:"), "{stderr}");
    assert!(stderr.contains("gcm: fallback message:"), "{stderr}");
    assert_plain(&stderr);
}

/// AC-12: a call that fails inside the provider, before any request goes out,
/// must leave nothing behind and wait for nothing. The non-TTY ticker interval
/// is 5s, so an uninterruptible wait would show up here as a 5s run.
#[test]
fn fast_failure_leaves_no_ticker() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 1);
    // A Groq key satisfies onboarding; OpenAI is then selected with no key of
    // its own, so `MissingKey` is raised inside `generate_plan` - after the
    // progress guard is already live.
    let url = stub(vec![]);

    let started = Instant::now();
    let out = run(
        repo.path(),
        cfg.path(),
        &url,
        &["--provider", "openai", "--dry-run", "--yes"],
    );
    let elapsed = started.elapsed();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "expected a failure: {stderr}");
    assert!(stderr.contains("OPENAI_API_KEY"), "{stderr}");
    assert!(
        !stderr.contains("still waiting"),
        "no ticker should have fired: {stderr}"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "the ticker wait was not interruptible: {elapsed:?}"
    );
    assert_plain(&stderr);
}

/// `GCM_LOG_LEVEL=off` is the documented opt-out, so it has to silence the whole
/// feature - status line, ticker, retry notices and the fallback announcement -
/// not just the lines that happen to go through the `log!` macro.
#[test]
fn log_level_off_silences_progress() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 2);
    let url = stub(vec![
        Act::Reply(Duration::ZERO, 400, context_window_400()),
        Act::Reply(Duration::ZERO, 200, message_body()),
    ]);

    let out = command(repo.path(), cfg.path(), &url, &["--yes"])
        .env("GCM_LOG_LEVEL", "off")
        .output()
        .expect("run gcm");
    let stderr = String::from_utf8_lossy(&out.stderr);
    for noise in ["gcm: grouping:", "Falling back", "still waiting"] {
        assert!(!stderr.contains(noise), "{noise:?} survived off: {stderr}");
    }
}

/// An absurd retry budget must not panic. `attempt of total` is computed in
/// `u64`, so `GCM_RETRY_MAX=u32::MAX` prints a silly number rather than
/// overflowing and taking the run down with it.
#[test]
fn extreme_retry_budget_does_not_overflow() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    repo_with_files(repo.path(), 1);
    let url = stub(vec![
        Act::Reply(Duration::ZERO, 429, "{}".to_string()),
        Act::Reply(Duration::ZERO, 200, plan_body(&["f0.txt"])),
    ]);

    let out = command(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"])
        .env("GCM_RETRY_MAX", "4294967295")
        .env("GCM_RETRY_BASE_MS", "1")
        .output()
        .expect("run gcm");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "run failed: {stderr}");
    assert!(stderr.contains("attempt 1 of 4294967296"), "{stderr}");
}
