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
    tree(dir, 515)
}

/// `n` untracked files shaped like [`large_tree`]'s.
fn tree(dir: &Path, n: usize) -> Vec<String> {
    tree_of(dir, n, 120)
}

/// `n` untracked files under [`large_tree`]'s paths, `lines` lines each.
fn tree_of(dir: &Path, n: usize, lines: usize) -> Vec<String> {
    git_init(dir);
    let base = "frontend/packages/design-system/src/components/navigation/sidebar/sections";
    let mut paths = Vec::new();
    for i in 0..n {
        let rel = format!(
            "{base}/group_{:02}/collapsible_section_item_{i:04}.tsx",
            i % 20
        );
        let full = dir.join(&rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        let content: String = (0..lines)
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

fn signing_available() -> bool {
    let dir = tempfile::tempdir().unwrap();
    git_init(dir.path());
    Command::new("git")
        .args(["commit", "-S", "--allow-empty", "-q", "-m", "probe"])
        .current_dir(dir.path())
        .status()
        .expect("run git")
        .success()
}

/// The one JSON envelope `--json` prints on stdout.
fn envelope(out: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "stdout is not one JSON envelope ({e}): {stdout}\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// The user message of a captured chat payload.
fn user_content(payload: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(payload).expect("JSON payload");
    v["messages"]
        .as_array()
        .and_then(|m| m.iter().find(|m| m["role"] == "user"))
        .and_then(|m| m["content"].as_str())
        .expect("a user message")
        .to_string()
}

/// AC2: every path is listed once, in the inventory; past the read cap the
/// untracked files share one trailer instead of a stub each.
#[test]
fn ac2_each_path_appears_once_in_one_inventory() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let paths = large_tree(repo.path());
    let (url, captured) = capture(vec![bad_request()]);

    run(groq(repo.path(), cfg.path(), &url, &["--dry-run", "--yes"]));
    let requests = captured.lock().unwrap().clone();
    let user = user_content(&requests[0]);
    let start = user.find(":\n").expect("inventory label") + 2;
    let end = user.find("\n\nDiff stats").expect("stat label");
    let inventory: Vec<serde_json::Value> =
        serde_json::from_str(&user[start..end]).expect("the inventory is one JSON array");
    assert_eq!(inventory.len(), paths.len());
    for path in &paths {
        let listed = inventory.iter().filter(|e| e["path"] == *path).count();
        assert_eq!(listed, 1, "{path} listed {listed} times");
    }
    for (i, entry) in inventory.iter().enumerate() {
        assert_eq!(entry["i"], i, "entry {i} carries its own index");
    }
    assert!(!user.contains("Git status"), "no second path list");
    assert_eq!(
        user.matches("further untracked files past the read cap")
            .count(),
        1,
        "one trailer for the files past the cap"
    );
}

/// A reduced grouping prompt is announced on stderr, and `--json` stdout is
/// still exactly one envelope.
#[test]
fn degradation_is_announced_on_stderr_only() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    large_tree(repo.path());
    let (url, _captured) = capture(vec![bad_request()]);

    let mut cmd = groq(repo.path(), cfg.path(), &url, &["--json", "--dry-run"]);
    cmd.env("GCM_CONTEXT_WINDOW_TOKENS", "100000");
    let out = run(cmd);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("did not fit its 100000-token context window"),
        "{stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("did not fit"), "{stdout}");
    envelope(&out);
}

/// AC4, both over budget: the grouping prompt cannot fit, the fallback message
/// cannot either, and gcm says so without sending a request.
#[test]
fn both_over_budget_is_one_error_and_no_request() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    large_tree(repo.path());
    let (url, captured) = capture(vec![bad_request()]);

    let mut cmd = groq(repo.path(), cfg.path(), &url, &["--json", "--dry-run"]);
    cmd.env("GCM_CONTEXT_WINDOW_TOKENS", "8192");
    let out = run(cmd);
    assert!(captured.lock().unwrap().is_empty(), "a request was sent");
    let env = envelope(&out);
    assert_eq!(env["status"], "error");
    assert_eq!(env["error"]["code"], "PromptTooLarge");
    let message = env["error"]["message"].as_str().unwrap();
    assert!(message.contains("fallback message"), "{message}");
    assert!(
        message.contains("no fallback message request was sent"),
        "{message}"
    );
}

/// `--all`: the single-commit message is checked before it is sent.
#[test]
fn single_commit_over_budget_sends_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    large_tree(repo.path());
    let (url, captured) = capture(vec![bad_request()]);

    let mut cmd = groq(
        repo.path(),
        cfg.path(),
        &url,
        &["--json", "--all", "--dry-run"],
    );
    cmd.env("GCM_CONTEXT_WINDOW_TOKENS", "8192");
    let out = run(cmd);
    assert!(captured.lock().unwrap().is_empty(), "a request was sent");
    let env = envelope(&out);
    assert_eq!(env["error"]["code"], "PromptTooLarge");
    assert!(env["error"]["message"]
        .as_str()
        .unwrap()
        .contains("single-commit message"));
}

/// An advanced cached plan's first group has no message, so gcm regenerates
/// it; over budget that call is refused before sending, and the cache and the
/// index are untouched.
#[test]
fn cached_group_message_over_budget_sends_nothing() {
    if !signing_available() {
        eprintln!("skipping: git commit -S is not available here");
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let paths = tree(repo.path(), 20);

    // Seed the cache through gcm itself: committing group 1 of a two-group plan
    // advances the cache to group 2, whose message the plan left null.
    let (first, second) = paths.split_at(10);
    let plan = serde_json::json!({
        "groups": [
            { "files": first, "summary": "first", "commit_message": "feat: first half" },
            { "files": second, "summary": "second", "commit_message": null }
        ]
    })
    .to_string();
    let reply = serde_json::json!({ "choices": [{ "message": { "content": plan } }] });
    let (url, _seed) = capture(vec![(200, reply.to_string())]);
    let mut seed = groq(repo.path(), cfg.path(), &url, &["--json", "--yes"]);
    seed.env("GCM_CACHE_DIR", cache.path());
    let seeded = run(seed);
    assert_eq!(
        envelope(&seeded)["status"],
        "committed",
        "seed run: {}",
        String::from_utf8_lossy(&seeded.stderr)
    );
    let snapshot = |dir: &Path| -> Vec<(std::path::PathBuf, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                let bytes = std::fs::read(&p).unwrap();
                (p, bytes)
            })
            .collect();
        files.sort();
        files
    };
    let cache_before = snapshot(cache.path());
    assert!(!cache_before.is_empty(), "the seed run saved a plan");
    let index = repo.path().join(".git/index");
    let index_before = std::fs::read(&index).ok();

    let (url, captured) = capture(vec![bad_request()]);
    let mut cmd = groq(repo.path(), cfg.path(), &url, &["--json", "--dry-run"]);
    cmd.env("GCM_CACHE_DIR", cache.path())
        .env("GCM_CONTEXT_WINDOW_TOKENS", "8192");
    let out = run(cmd);
    assert!(captured.lock().unwrap().is_empty(), "a request was sent");
    let env = envelope(&out);
    assert_eq!(env["error"]["code"], "PromptTooLarge");
    assert_eq!(env["mode"], "grouped");
    assert!(env["error"]["message"]
        .as_str()
        .unwrap()
        .contains("group message"));
    assert_eq!(snapshot(cache.path()), cache_before, "cache changed");
    assert_eq!(std::fs::read(&index).ok(), index_before, "index changed");
}

/// An Anthropic command against the capture listener at `url`.
fn anthropic(repo: &Path, cfg: &Path, url: &str, args: &[&str]) -> Command {
    let mut cmd = command(repo, cfg, args);
    cmd.env("ANTHROPIC_API_KEY", "sk-test")
        .env("GCM_PROVIDER", "anthropic")
        .env("GCM_MODEL", "claude-haiku-4-5")
        .env("GCM_ANTHROPIC_BASE_URL", url);
    cmd
}

/// An Anthropic `tool_use` reply whose input is `plan`.
fn anthropic_plan_reply(plan: serde_json::Value) -> (u16, String) {
    let body = serde_json::json!({
        "id": "msg_test",
        "type": "message",
        "role": "assistant",
        "model": "claude-haiku-4-5",
        "content": [{ "type": "tool_use", "id": "toolu_test", "name": "commit_plan", "input": plan }],
        "stop_reason": "tool_use"
    });
    (200, body.to_string())
}

/// Anthropic caps the plan reply at 4096 tokens. 515 echoed paths exceeded it;
/// 515 indices fit (CLO-801), so the plan call is made and an integer-index
/// reply groups the change set.
#[test]
fn anthropic_515_files_send_one_plan_call_and_group_by_index() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let paths = large_tree(repo.path());
    let first: Vec<usize> = (0..300).collect();
    let second: Vec<usize> = (300..paths.len()).collect();
    let plan = serde_json::json!({ "groups": [
        { "files": first, "summary": "first sections", "commit_message": "feat: add the first sidebar sections" },
        { "files": second, "summary": "second sections", "commit_message": null }
    ]});
    let (url, captured) = capture(vec![anthropic_plan_reply(plan)]);

    let out = run(anthropic(
        repo.path(),
        cfg.path(),
        &url,
        &["--json", "--dry-run", "--yes"],
    ));
    let requests = captured.lock().unwrap().clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(requests.len(), 1, "one request, the plan call: {stderr}");
    let payload: serde_json::Value = serde_json::from_str(&requests[0]).unwrap();
    assert!(payload.get("tools").is_some(), "the plan call");
    assert_eq!(payload["max_tokens"], 4096);
    let user = payload["messages"][0]["content"].as_str().unwrap();
    let start = user.find(":\n").expect("inventory label") + 2;
    let end = user.find("\n\nDiff stats").expect("stat label");
    let inventory: Vec<serde_json::Value> =
        serde_json::from_str(&user[start..end]).expect("the inventory is one JSON array");
    assert_eq!(inventory.len(), paths.len());
    let env = envelope(&out);
    assert_eq!(env["status"], "plan", "{env}");
    let groups = env["plan"]["groups"].as_array().expect("groups");
    assert_eq!(groups.len(), 2, "{env}");
    let resolved: Vec<&serde_json::Value> = groups
        .iter()
        .flat_map(|g| g["files"].as_array().unwrap())
        .collect();
    assert_eq!(groups[0]["files"].as_array().unwrap().len(), 300);
    for (i, path) in resolved.iter().enumerate() {
        assert_eq!(**path, inventory[i]["path"], "index {i} maps to its entry");
    }
}

/// 2,000 indices still exceed Anthropic's 4096-token plan cap: no plan call,
/// and the refusal names the cap.
#[test]
fn anthropic_2000_files_refuse_the_plan_call() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    tree_of(repo.path(), 2_000, 1);
    let reply = serde_json::json!({
        "id": "msg_test",
        "type": "message",
        "role": "assistant",
        "model": "claude-haiku-4-5",
        "content": [{ "type": "text", "text": "feat: add the sidebar sections" }],
        "stop_reason": "end_turn"
    })
    .to_string();
    let (url, captured) = capture(vec![(200, reply)]);

    let out = run(anthropic(
        repo.path(),
        cfg.path(),
        &url,
        &["--json", "--dry-run"],
    ));
    let requests = captured.lock().unwrap().clone();
    for request in &requests {
        let payload: serde_json::Value = serde_json::from_str(request).unwrap();
        assert!(payload.get("tools").is_none(), "no plan call was sent");
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("plan output cap"), "{stderr}");
    assert!(stderr.contains("file indices"), "{stderr}");
}

/// The reply-shape line counts the integer and string references of the reply
/// that produced the plan, at debug level only.
#[test]
fn plan_reply_ref_counts_are_logged_at_debug_level() {
    let run_with = |level: Option<&str>| {
        let repo = tempfile::tempdir().unwrap();
        let cfg = tempfile::tempdir().unwrap();
        let paths = tree_of(repo.path(), 3, 1);
        let plan = serde_json::json!({ "groups": [
            { "files": [0, paths[1], 2], "summary": "s", "commit_message": "feat: add sections" }
        ]});
        let (url, _captured) = capture(vec![groq_reply(&plan.to_string())]);
        let mut cmd = groq(repo.path(), cfg.path(), &url, &["--json", "--dry-run"]);
        if let Some(level) = level {
            cmd.env("GCM_LOG_LEVEL", level);
        }
        let out = run(cmd);
        assert_eq!(envelope(&out)["status"], "plan");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    let debug = run_with(Some("debug"));
    assert!(
        debug.contains("plan reply: 2 index refs, 1 path refs"),
        "{debug}"
    );
    let default = run_with(None);
    assert!(!default.contains("plan reply:"), "{default}");
}

/// A Groq chat reply whose content is `text`.
fn groq_reply(text: &str) -> (u16, String) {
    let body = serde_json::json!({ "choices": [{ "message": { "content": text } }] });
    (200, body.to_string())
}

/// The message check uses the generous estimate: the 515-file `--all` message,
/// ~144K tokens at 2 bytes per token, is ~72K at 4 and reaches the provider.
#[test]
fn single_commit_message_under_the_generous_estimate_is_sent() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    large_tree(repo.path());
    let (url, captured) = capture(vec![groq_reply("feat: add the sidebar sections")]);

    let out = run(groq(
        repo.path(),
        cfg.path(),
        &url,
        &["--json", "--all", "--dry-run"],
    ));
    assert_eq!(captured.lock().unwrap().len(), 1, "one message request");
    let env = envelope(&out);
    assert_eq!(env["status"], "plan", "{env}");
    assert_eq!(env["mode"], "single");
}

/// An invalid plan for the 515-file tree still ends in a sent, successful
/// fallback message rather than a pre-flight refusal.
#[test]
fn invalid_plan_falls_back_to_a_sent_message() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    large_tree(repo.path());
    let plan = serde_json::json!({
        "groups": [{ "files": [], "summary": "empty", "commit_message": "feat: nothing" }]
    })
    .to_string();
    let (url, captured) = capture(vec![
        groq_reply(&plan),
        groq_reply("feat: add the sidebar sections"),
    ]);

    let out = run(groq(
        repo.path(),
        cfg.path(),
        &url,
        &["--json", "--dry-run"],
    ));
    let requests = captured.lock().unwrap().clone();
    assert_eq!(
        requests.len(),
        2,
        "the plan call, then the fallback message"
    );
    let message: serde_json::Value = serde_json::from_str(&requests[1]).unwrap();
    assert!(
        message.get("response_format").is_none(),
        "not the plan call"
    );
    let env = envelope(&out);
    assert_eq!(env["status"], "plan", "{env}");
    assert_eq!(env["mode"], "single");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Falling back to single-commit mode"),
        "{stderr}"
    );
}

/// A message the generous estimate lets through can still overflow the real
/// window; the provider's context-window error keeps its recovery advice.
#[test]
fn provider_context_window_error_stays_actionable() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    large_tree(repo.path());
    let body = serde_json::json!({
        "error": { "message": "Request too large", "code": "context_length_exceeded" }
    });
    let (url, captured) = capture(vec![(400, body.to_string())]);

    let out = run(groq(
        repo.path(),
        cfg.path(),
        &url,
        &["--json", "--all", "--dry-run"],
    ));
    assert_eq!(captured.lock().unwrap().len(), 1, "one message request");
    let env = envelope(&out);
    assert_eq!(env["status"], "error", "{env}");
    assert_ne!(env["error"]["code"], "PromptTooLarge");
    let message = env["error"]["message"].as_str().unwrap();
    assert!(message.contains("too large for this model"), "{message}");
    assert!(message.contains("single-commit message"), "{message}");
}

/// The grouping prompt cannot fit even as inventory and stat, while the fallback
/// message, a line per file, fits the generous estimate: the plan call is
/// skipped and the one request is a message that succeeds.
#[test]
fn grouping_cannot_fit_falls_back_to_a_sent_message() {
    let repo = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    tree_of(repo.path(), 515, 1);
    let (url, captured) = capture(vec![groq_reply("feat: add the sidebar sections")]);

    let mut cmd = groq(repo.path(), cfg.path(), &url, &["--json", "--dry-run"]);
    cmd.env("GCM_CONTEXT_WINDOW_TOKENS", "35000");
    let out = run(cmd);
    let requests = captured.lock().unwrap().clone();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(requests.len(), 1, "only the fallback message: {stderr}");
    let message: serde_json::Value = serde_json::from_str(&requests[0]).unwrap();
    assert!(
        message.get("response_format").is_none(),
        "not the plan call"
    );
    let env = envelope(&out);
    assert_eq!(env["status"], "plan", "{env}");
    assert_eq!(env["mode"], "single");
    assert!(stderr.contains("no grouping request was sent"), "{stderr}");
    assert!(
        stderr.contains("Falling back to single-commit mode"),
        "{stderr}"
    );
}
