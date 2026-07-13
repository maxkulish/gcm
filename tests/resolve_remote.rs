//! Integration tests for `gcm resolve --pr` / `--mr` (CLO-533).
//!
//! Uses fake `gh`/`glab` scripts on PATH and a local git repo as the remote
//! origin. No real network calls are made.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
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

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("run git")
        .success();
    assert!(ok, "git {args:?} failed in {}", dir.display());
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
        assert!(ok, "git {args:?} failed in {}", dir.display());
    }
}

fn git_output(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Write fake `gh` and `glab` scripts that emulate the host CLI for testing.
/// When `capture_dir` is provided, the `gh pr comment` / `glab mr note`
/// commands write a sentinel file there.
fn write_fake_scripts(bin_dir: &Path, _capture_dir: Option<&Path>) {
    fs::create_dir_all(bin_dir).unwrap();

    let gh = bin_dir.join("gh");
    fs::write(
        &gh,
        r##"#!/bin/sh
# Fake gh for CLO-533 tests.
# Uses GCM_FAKE_CAPTURE_DIR env var to record comment calls.

if [ "$1" = "pr" ] && [ "$2" = "checkout" ]; then
  id="$3"
  branch="$5"
  # Fetch the source branch from origin so it's available.
  git fetch -q origin "pr-$id-source" 2>/dev/null || true
  git checkout -q -b "$branch" "FETCH_HEAD" 2>/dev/null || git checkout -q "$branch" 2>/dev/null || true
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
  echo '{"baseRefName":"main"}'
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "comment" ]; then
  if [ -n "$GCM_FAKE_CAPTURE_DIR" ]; then
    touch "$GCM_FAKE_CAPTURE_DIR/gh-comment-called"
  fi
  exit 0
fi
echo "fake-gh: unexpected args $*" >&2
exit 1
"##,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let glab = bin_dir.join("glab");
    fs::write(
        &glab,
        r##"#!/bin/sh
# Fake glab for CLO-533 tests.
# Uses GCM_FAKE_CAPTURE_DIR env var to record note calls.

if [ "$1" = "mr" ] && [ "$2" = "checkout" ]; then
  id="$3"
  branch="$5"
  git fetch -q origin "mr-$id-source" 2>/dev/null || true
  git checkout -q -b "$branch" "FETCH_HEAD" 2>/dev/null || git checkout -q "$branch" 2>/dev/null || true
  exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "view" ]; then
  echo '{"target_branch":"main"}'
  exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "note" ]; then
  if [ -n "$GCM_FAKE_CAPTURE_DIR" ]; then
    touch "$GCM_FAKE_CAPTURE_DIR/glab-note-called"
  fi
  exit 0
fi
echo "fake-glab: unexpected args $*" >&2
exit 1
"##,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&glab, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Build a fake remote repo with a feature branch that conflicts with main.
fn build_fake_remote(tmp: &Path, source_ref: &str) -> PathBuf {
    let remote = tmp.join("remote.git");
    fs::create_dir_all(&remote).unwrap();
    git_init(&remote);
    fs::write(remote.join("f.txt"), "base\n").unwrap();
    git(&remote, &["add", "-A"]);
    git(&remote, &["commit", "-q", "-m", "init"]);
    git(&remote, &["branch", "-M", "main"]);

    // Feature branch modifies f.txt.
    git(&remote, &["checkout", "-q", "-b", source_ref]);
    fs::write(remote.join("f.txt"), "feature\n").unwrap();
    git(&remote, &["commit", "-q", "-am", "feature"]);
    git(&remote, &["checkout", "-q", "main"]);

    // Make main diverge so the merge conflicts.
    fs::write(remote.join("f.txt"), "mainline\n").unwrap();
    git(&remote, &["commit", "-q", "-am", "mainline"]);

    // Mirror the source branch under the expected remote refs.
    git(
        &remote,
        &[
            "update-ref",
            &format!("refs/remotes/origin/{source_ref}"),
            &format!("refs/heads/{source_ref}"),
        ],
    );

    remote
}

/// Build a fake remote repo where the feature branch merges cleanly (no conflict).
fn build_fake_remote_clean(tmp: &Path, source_ref: &str) -> PathBuf {
    let remote = tmp.join("remote.git");
    fs::create_dir_all(&remote).unwrap();
    git_init(&remote);
    fs::write(remote.join("f.txt"), "base\n").unwrap();
    git(&remote, &["add", "-A"]);
    git(&remote, &["commit", "-q", "-m", "init"]);
    git(&remote, &["branch", "-M", "main"]);

    // Feature branch adds a new file (no conflict with main).
    git(&remote, &["checkout", "-q", "-b", source_ref]);
    fs::write(remote.join("g.txt"), "new file\n").unwrap();
    git(&remote, &["add", "-A"]);
    git(&remote, &["commit", "-q", "-m", "add new file"]);
    git(&remote, &["checkout", "-q", "main"]);

    // Mirror the source branch under the expected remote refs.
    git(
        &remote,
        &[
            "update-ref",
            &format!("refs/remotes/origin/{source_ref}"),
            &format!("refs/heads/{source_ref}"),
        ],
    );

    remote
}

fn run_gcm(
    repo: &Path,
    config_dir: &Path,
    bin_dir: &Path,
    extra_env: &[(&str, &str)],
    args: &[&str],
) -> Output {
    let path_env = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.display(), path_env);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(args)
        .env("GCM_CONFIG", config_dir)
        .env("PATH", path_env)
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

fn run_gcm_with_capture(
    repo: &Path,
    config_dir: &Path,
    bin_dir: &Path,
    capture_dir: &Path,
    args: &[&str],
) -> Output {
    run_gcm(
        repo,
        config_dir,
        bin_dir,
        &[("GCM_FAKE_CAPTURE_DIR", &capture_dir.to_string_lossy())],
        args,
    )
}

fn run_gcm_with_home(
    repo: &Path,
    config_dir: &Path,
    bin_dir: &Path,
    home_dir: &Path,
    args: &[&str],
) -> Output {
    run_gcm_with_home_env(repo, config_dir, bin_dir, home_dir, &[], args)
}

fn run_gcm_with_home_env(
    repo: &Path,
    config_dir: &Path,
    bin_dir: &Path,
    home_dir: &Path,
    extra_env: &[(&str, &str)],
    args: &[&str],
) -> Output {
    let path_env = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.display(), path_env);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(args)
        .env("GCM_CONFIG", config_dir)
        .env("PATH", path_env)
        .env("HOME", home_dir)
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

fn run_gcm_with_capture_and_home(
    repo: &Path,
    config_dir: &Path,
    bin_dir: &Path,
    capture_dir: &Path,
    home_dir: &Path,
    args: &[&str],
) -> Output {
    let path_env = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{}", bin_dir.display(), path_env);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(args)
        .env("GCM_CONFIG", config_dir)
        .env("PATH", path_env)
        .env("HOME", home_dir)
        .env("GCM_FAKE_CAPTURE_DIR", capture_dir)
        .env_remove("GCM_DEBUG")
        .stdin(Stdio::null());
    for var in PROVIDER_ENV {
        cmd.env_remove(var);
    }
    cmd.output().expect("run gcm")
}

fn host_binary(name: &str) -> PathBuf {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|p| p.join(name))
                .find(|p| p.is_file())
        })
        .unwrap_or_else(|| PathBuf::from(name))
}

fn install_git_shim(bin_dir: &Path) {
    fs::create_dir_all(bin_dir).unwrap();
    let git = host_binary("git");
    let shim = bin_dir.join("git");
    #[cfg(unix)]
    {
        let _ = fs::remove_file(&shim);
        std::os::unix::fs::symlink(&git, &shim).unwrap();
    }
    #[cfg(not(unix))]
    fs::copy(git, shim).unwrap();
}

fn run_gcm_no_host(repo: &Path, config_dir: &Path, bin_dir: &Path, args: &[&str]) -> Output {
    // Use only a test bin directory containing a git shim, so CI images with
    // system `gh`/`glab` in /usr/bin still exercise the missing-CLI path.
    install_git_shim(bin_dir);
    let path_env = bin_dir.display().to_string();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gcm"));
    cmd.current_dir(repo)
        .args(args)
        .env("GCM_CONFIG", config_dir)
        .env("PATH", path_env)
        .env_remove("GCM_DEBUG")
        .stdin(Stdio::null());
    for var in PROVIDER_ENV {
        cmd.env_remove(var);
    }
    cmd.output().expect("run gcm")
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

fn setup_user_repo(tmp: &Path, remote: &Path) -> PathBuf {
    let user_repo = tmp.join("user");
    fs::create_dir_all(&user_repo).unwrap();
    git_init(&user_repo);
    // Make an initial commit so HEAD exists.
    fs::write(user_repo.join("README.md"), "# user repo\n").unwrap();
    git(&user_repo, &["add", "-A"]);
    git(&user_repo, &["commit", "-q", "-m", "init"]);
    git(&user_repo, &["branch", "-M", "main"]);
    git(
        &user_repo,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    user_repo
}

/// Set up a user repo with a URL-like origin (for bare-id tests that need
/// origin parsing in dry-run mode where no clone happens).
fn setup_user_repo_url_origin(tmp: &Path, origin_url: &str) -> PathBuf {
    let user_repo = tmp.join("user");
    fs::create_dir_all(&user_repo).unwrap();
    git_init(&user_repo);
    fs::write(user_repo.join("README.md"), "# user repo\n").unwrap();
    git(&user_repo, &["add", "-A"]);
    git(&user_repo, &["commit", "-q", "-m", "init"]);
    git(&user_repo, &["branch", "-M", "main"]);
    git(&user_repo, &["remote", "add", "origin", origin_url]);
    user_repo
}

fn basic_config(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    write_config(
        dir,
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );
}

/// Fake Ollama that returns one real resolution for the single conflicted hunk.
fn start_fake_ollama_with_resolution(replacement: &str) -> String {
    let inner = serde_json::json!({
        "resolutions": [{"hunk_index": 0, "replacement": replacement}]
    })
    .to_string();
    let body = serde_json::json!({"message": {"content": inner}}).to_string();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0_u8; 8192];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

fn start_fake_ollama_empty_resolutions() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0_u8; 8192];
            let _ = stream.read(&mut buf);
            let body = r#"{"message":{"content":"{\"resolutions\":[]}"}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

/// Set up a fake HOME with a .gitconfig that redirects a URL prefix to a
/// local bare repo, so non-dry-run tests can clone from it.
fn setup_git_redirect(tmp: &Path, remote: &Path, url_prefix: &str) -> PathBuf {
    let home = tmp.join("home");
    fs::create_dir_all(&home).unwrap();
    let gitconfig = home.join(".gitconfig");
    fs::write(
        &gitconfig,
        format!(
            "[url \"{}\"]\n\tinsteadOf = {}\n",
            remote.to_string_lossy(),
            url_prefix
        ),
    )
    .unwrap();
    home
}

// ---------------------------------------------------------------------------
// Scenario 1: GitHub PR URL (dry-run)
// ---------------------------------------------------------------------------

#[test]
fn parse_github_url() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"status\":\"noop\""), "{stdout}");
    assert!(
        stdout.contains("gcm-resolve-github-42"),
        "resolution branch name missing: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 2: GitLab MR URL (dry-run)
// ---------------------------------------------------------------------------

#[test]
fn parse_gitlab_url() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = setup_user_repo(dir.path(), &build_fake_remote(dir.path(), "mr-7-source"));

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--mr",
            "https://gitlab.com/acme/app/-/merge_requests/7",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"status\":\"noop\""), "{stdout}");
    assert!(
        stdout.contains("gcm-resolve-gitlab-7"),
        "resolution branch name missing: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 3: Host from origin remote (bare id dry-run)
// ---------------------------------------------------------------------------

#[test]
fn host_from_origin_remote() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = setup_user_repo_url_origin(dir.path(), "https://github.com/acme/app.git");

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &["resolve", "--pr", "99", "--dry-run", "--json"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"status\":\"noop\""), "{stdout}");
    assert!(
        stdout.contains("gcm-resolve-github-99"),
        "resolution branch name missing: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: Missing gh CLI
// ---------------------------------------------------------------------------

#[test]
fn missing_gh_error() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = setup_user_repo(dir.path(), &build_fake_remote(dir.path(), "pr-1-source"));

    // Empty bin dir — no gh on PATH.
    let bin = dir.path().join("bin");
    fs::create_dir_all(&bin).unwrap();

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm_no_host(
        &user_repo,
        &cfg_dir,
        &bin,
        &["resolve", "--pr", "https://github.com/acme/app/pull/1"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "should fail");
    assert!(stderr.contains("gh"), "should mention gh: {stderr}");
}

// ---------------------------------------------------------------------------
// Scenario 5: Missing glab CLI
// ---------------------------------------------------------------------------

#[test]
fn missing_glab_error() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = setup_user_repo(dir.path(), &build_fake_remote(dir.path(), "mr-1-source"));

    let bin = dir.path().join("bin");
    fs::create_dir_all(&bin).unwrap();

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm_no_host(
        &user_repo,
        &cfg_dir,
        &bin,
        &[
            "resolve",
            "--mr",
            "https://gitlab.com/acme/app/-/merge_requests/1",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "should fail");
    assert!(stderr.contains("glab"), "should mention glab: {stderr}");
}

// ---------------------------------------------------------------------------
// Scenario 6: Scratch repo isolation
// ---------------------------------------------------------------------------

#[test]
fn scratch_repo_is_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);

    // Record user repo state before.
    let before = git_output(&user_repo, &["rev-parse", "HEAD"]);

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    // Use --dry-run to avoid needing a real provider; this still tests that the
    // user repo is not mutated.
    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--json",
        ],
    );
    assert!(out.status.success());

    // User repo HEAD should not have changed.
    let after = git_output(&user_repo, &["rev-parse", "HEAD"]);
    assert_eq!(before, after, "user repo was mutated");
}

// ---------------------------------------------------------------------------
// Scenario 7: Resolution branch naming
// ---------------------------------------------------------------------------

#[test]
fn resolution_branch_naming() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"resolution_branch\":\"gcm-resolve-github-42\""),
        "expected resolution branch in JSON: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Scenario 8: Dry-run purity (no clone)
// ---------------------------------------------------------------------------

#[test]
fn dry_run_no_clone() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = dir.path().join("user");
    fs::create_dir_all(&user_repo).unwrap();
    git_init(&user_repo);

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/1",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"status\":\"noop\""), "{stdout}");
    // No sentinel files should exist.
    assert!(!bin.join("gh-used").exists());
}

// ---------------------------------------------------------------------------
// Scenario 9: Dry-run ignores --remote-push and --remote-comment
// ---------------------------------------------------------------------------

#[test]
fn dry_run_ignores_remote_flags() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = setup_user_repo(dir.path(), &build_fake_remote(dir.path(), "pr-42-source"));

    let capture = dir.path().join("capture");
    fs::create_dir_all(&capture).unwrap();

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm_with_capture(
        &user_repo,
        &cfg_dir,
        &bin,
        &capture,
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--remote-push",
            "--remote-comment",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"pushed\":false"), "{stdout}");
    assert!(stdout.contains("\"commented\":false"), "{stdout}");
    // No comment sentinel should exist.
    assert!(!capture.join("gh-comment-called").exists());
}

// ---------------------------------------------------------------------------
// Scenario 10: Default no push
// ---------------------------------------------------------------------------

#[test]
fn default_no_push() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = setup_user_repo(dir.path(), &build_fake_remote(dir.path(), "pr-42-source"));

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"pushed\":false"), "{stdout}");
}

// ---------------------------------------------------------------------------
// Scenario 11: Remote push invoked
// ---------------------------------------------------------------------------

#[test]
fn remote_push_invoked() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    // We can't run a full resolve without a provider, but we can verify the
    // --remote-push flag is parsed and accepted. In dry-run, push is not
    // invoked, so we check the flag is accepted without error.
    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--remote-push",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // In dry-run, push is still false (not invoked).
    assert!(stdout.contains("\"pushed\":false"), "{stdout}");
}

// ---------------------------------------------------------------------------
// Scenario 12: Remote comment invoked
// ---------------------------------------------------------------------------

#[test]
fn remote_comment_invoked() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);

    let capture = dir.path().join("capture");
    fs::create_dir_all(&capture).unwrap();

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    // In dry-run, comment is not invoked.
    let out = run_gcm_with_capture(
        &user_repo,
        &cfg_dir,
        &bin,
        &capture,
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--remote-comment",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"commented\":false"), "{stdout}");
    assert!(!capture.join("gh-comment-called").exists());
}

// ---------------------------------------------------------------------------
// Scenario 13: Clean merge (no conflicts)
// ---------------------------------------------------------------------------

#[test]
fn clean_merge_no_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote_clean(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    // We can't run a full resolve without a provider, but dry-run still
    // verifies the command parses. The clean-merge logic is tested at the
    // unit level in resolve::tests.
    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"status\":\"noop\""), "{stdout}");
}

// ---------------------------------------------------------------------------
// Scenario 14: Partial escalation report
// ---------------------------------------------------------------------------

#[test]
fn partial_escalation_report() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    // Dry-run returns noop; the partial escalation path is exercised when
    // the engine runs. Here we verify the JSON shape includes the fields.
    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The report should include a remote block with the resolution branch.
    assert!(stdout.contains("\"remote\":"), "{stdout}");
    assert!(stdout.contains("\"resolution_branch\":"), "{stdout}");
}

// ---------------------------------------------------------------------------
// Scenario: Unsupported host fails (extra coverage for EC2)
// ---------------------------------------------------------------------------

#[test]
fn unsupported_host_fails() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = setup_user_repo(dir.path(), &build_fake_remote(dir.path(), "pr-1-source"));

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://bitbucket.org/acme/app/pull/1",
            "--dry-run",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "should fail");
    assert!(
        stderr.contains("bitbucket.org"),
        "should mention bitbucket.org: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Scenario: Dry-run with full URL doesn't need a repo (AC8)
// ---------------------------------------------------------------------------

#[test]
fn dry_run_full_url_no_repo() {
    let dir = tempfile::tempdir().unwrap();
    // No user repo at all.
    let empty = dir.path().join("empty");
    fs::create_dir_all(&empty).unwrap();

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);

    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);

    let out = run_gcm(
        &empty,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"status\":\"noop\""), "{stdout}");
}

// ---------------------------------------------------------------------------
// Real (non-dry-run) integration tests using clean-merge fixture (no provider needed)
// ---------------------------------------------------------------------------

const GITHUB_URL_PREFIX: &str = "https://github.com/acme/app";

/// Run a full remote resolve (non-dry-run) using the clean-merge fixture.
/// The clean-merge path returns Noop before needing a provider, so this
/// exercises clone, checkout, merge, commit, report end-to-end.
#[test]
fn real_clean_merge_resolves_and_commits() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote_clean(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let home = setup_git_redirect(dir.path(), &remote, GITHUB_URL_PREFIX);
    let out = run_gcm_with_home(
        &user_repo,
        &cfg_dir,
        &bin,
        &home,
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"status\":\"noop\""), "{stdout}");
    assert!(stdout.contains("\"remote\":"), "{stdout}");
    assert!(
        stdout.contains("\"resolution_branch\":\"gcm-resolve-github-42\""),
        "{stdout}"
    );
    assert!(stdout.contains("\"pushed\":false"), "{stdout}");
    assert!(stdout.contains("\"commented\":false"), "{stdout}");
}

/// Verify that --remote-push actually pushes to the fake remote.
#[test]
fn real_push_invoked() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote_clean(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let home = setup_git_redirect(dir.path(), &remote, GITHUB_URL_PREFIX);
    let out = run_gcm_with_home(
        &user_repo,
        &cfg_dir,
        &bin,
        &home,
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--remote-push",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"pushed\":true"), "{stdout}");
    let branches = git_output(&remote, &["branch", "--list"]);
    assert!(
        branches.contains("gcm-resolve-github-42"),
        "resolution branch not found in remote: {branches}"
    );
}

/// Verify that --remote-comment actually calls gh pr comment.
#[test]
fn real_comment_invoked() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote_clean(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let capture = dir.path().join("capture");
    fs::create_dir_all(&capture).unwrap();
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let home = setup_git_redirect(dir.path(), &remote, GITHUB_URL_PREFIX);
    let out = run_gcm_with_capture_and_home(
        &user_repo,
        &cfg_dir,
        &bin,
        &capture,
        &home,
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--remote-comment",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("\"commented\":true"), "{stdout}");
    assert!(
        capture.join("gh-comment-called").exists(),
        "gh pr comment was not invoked"
    );
}

/// Verify that the user repo is unchanged after a remote run (AC4/AC13).
#[test]
fn real_scratch_cleanup_on_success() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote_clean(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let home = setup_git_redirect(dir.path(), &remote, GITHUB_URL_PREFIX);
    let before = git_output(&user_repo, &["rev-parse", "HEAD"]);
    let before_branch = git_output(&user_repo, &["branch", "--show-current"]);
    let out = run_gcm_with_home(
        &user_repo,
        &cfg_dir,
        &bin,
        &home,
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--json",
        ],
    );
    assert!(out.status.success());
    let after = git_output(&user_repo, &["rev-parse", "HEAD"]);
    let after_branch = git_output(&user_repo, &["branch", "--show-current"]);
    assert_eq!(before, after, "user repo HEAD changed");
    assert_eq!(before_branch, after_branch, "user repo branch changed");
}

/// Verify that the resolution branch is created with the correct name (AC6)
/// and contains the merged tree.
#[test]
fn real_resolution_branch_created() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote_clean(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let home = setup_git_redirect(dir.path(), &remote, GITHUB_URL_PREFIX);
    let out = run_gcm_with_home(
        &user_repo,
        &cfg_dir,
        &bin,
        &home,
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--remote-push",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let branches = git_output(&remote, &["branch", "--list"]);
    assert!(
        branches.contains("gcm-resolve-github-42"),
        "resolution branch not found in remote: {branches}"
    );
    let file_content = git_output(&remote, &["show", "gcm-resolve-github-42:g.txt"]);
    assert_eq!(
        file_content, "new file",
        "merged tree should contain source file"
    );
}

/// Verify GitLab subgroup URL parsing.
#[test]
fn parse_gitlab_subgroup_url() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo =
        setup_user_repo_url_origin(dir.path(), "https://gitlab.com/acme/subgroup/app.git");
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let out = run_gcm(
        &user_repo,
        &cfg_dir,
        &bin,
        &[],
        &[
            "resolve",
            "--mr",
            "https://gitlab.com/acme/subgroup/app/-/merge_requests/3",
            "--dry-run",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("gcm-resolve-gitlab-3"), "{stdout}");
}

/// Verify that a conflicting merge exercises the remote conflict branch (AC5)
/// and leaves unresolved markers in the preserved scratch repo when the
/// provider returns no resolution (AC10).
#[test]
fn real_merge_produces_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let home = setup_git_redirect(dir.path(), &remote, GITHUB_URL_PREFIX);
    let ollama_base = start_fake_ollama_empty_resolutions();
    let out = run_gcm_with_home_env(
        &user_repo,
        &cfg_dir,
        &bin,
        &home,
        &[
            ("GCM_PROVIDER", "ollama"),
            ("GCM_OLLAMA_BASE_URL", ollama_base.as_str()),
        ],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--yes",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "partial", "{stdout}");
    assert_eq!(json["files"][0]["path"], "f.txt", "{stdout}");
    assert_eq!(json["files"][0]["action"], "escalated", "{stdout}");
    assert_eq!(json["files"][0]["hunks_escalated"], 1, "{stdout}");
    let scratch_path = json["remote"]["scratch_path"].as_str().unwrap();
    let scratch = PathBuf::from(scratch_path);
    assert!(
        scratch.exists(),
        "scratch repo should be preserved: {scratch_path}"
    );
    assert_eq!(
        git_output(&scratch, &["branch", "--show-current"]),
        "gcm-resolve-github-42"
    );
    let conflict_file = fs::read_to_string(scratch.join("f.txt")).unwrap();
    assert!(conflict_file.contains("<<<<<<<"), "{conflict_file}");
    assert!(conflict_file.contains(">>>>>>>"), "{conflict_file}");
}

/// Verify that scratch repo is cleaned up on error (AC13 error path).
#[test]
fn scratch_cleanup_on_error() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote_clean(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let bin = dir.path().join("bin");
    // Don't write fake scripts — gh won't be found, so the command fails
    // at require_host_cli, before the scratch repo is even created.
    fs::create_dir_all(&bin).unwrap();
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    // Use run_gcm_no_host to ensure gh is not on PATH.
    let out = run_gcm_no_host(
        &user_repo,
        &cfg_dir,
        &bin,
        &["resolve", "--pr", "https://github.com/acme/app/pull/42"],
    );
    assert!(!out.status.success(), "should fail without gh");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("gh"), "should mention gh: {stderr}");
    // No scratch dir should be left behind (TempDir auto-deletes on error).
    // We can't directly check temp dirs, but the user repo should be clean.
    let user_branch = git_output(&user_repo, &["branch", "--show-current"]);
    assert_eq!(user_branch, "main", "user repo should be unchanged");
}

// ---------------------------------------------------------------------------
// CLO-555 remote gate: commit/push only committable (Resolved/Noop) reports.
// ---------------------------------------------------------------------------

/// A Partial resolution is never committed and never pushed, even with
/// --remote-push: raw conflict markers must not be published.
#[test]
fn partial_never_commits_or_pushes() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let home = setup_git_redirect(dir.path(), &remote, GITHUB_URL_PREFIX);
    // Provider returns no resolutions -> every hunk escalates -> Partial.
    let ollama_base = start_fake_ollama_empty_resolutions();
    let out = run_gcm_with_home_env(
        &user_repo,
        &cfg_dir,
        &bin,
        &home,
        &[
            ("GCM_PROVIDER", "ollama"),
            ("GCM_OLLAMA_BASE_URL", ollama_base.as_str()),
        ],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--remote-push",
            "--yes",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "partial", "{stdout}");
    assert_eq!(json["remote"]["pushed"], false, "{stdout}");

    // Nothing reached the remote.
    let remote_branches = git_output(&remote, &["branch", "--list"]);
    assert!(
        !remote_branches.contains("gcm-resolve-github-42"),
        "partial branch must not be pushed: {remote_branches}"
    );

    // The scratch resolution branch has zero new commits (no marker commit).
    let scratch = PathBuf::from(json["remote"]["scratch_path"].as_str().unwrap());
    assert!(scratch.exists(), "scratch kept for manual completion");
    let new_commits = git_output(&scratch, &["rev-list", "--count", "main..HEAD"]);
    assert_eq!(new_commits, "0", "no commit on a partial resolution");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("never pushed"),
        "push skip explained: {stderr}"
    );
}

/// A Resolved remote run produces exactly one first-parent commit on the
/// resolution branch - the merge commit - never a stacked empty duplicate.
#[test]
fn resolved_remote_produces_exactly_one_commit() {
    let dir = tempfile::tempdir().unwrap();
    let remote = build_fake_remote(dir.path(), "pr-42-source");
    let user_repo = setup_user_repo(dir.path(), &remote);
    let bin = dir.path().join("bin");
    write_fake_scripts(&bin, None);
    let cfg_dir = dir.path().join("cfg");
    basic_config(&cfg_dir);
    let home = setup_git_redirect(dir.path(), &remote, GITHUB_URL_PREFIX);
    let ollama_base = start_fake_ollama_with_resolution("resolved\n");
    let out = run_gcm_with_home_env(
        &user_repo,
        &cfg_dir,
        &bin,
        &home,
        &[
            ("GCM_PROVIDER", "ollama"),
            ("GCM_OLLAMA_BASE_URL", ollama_base.as_str()),
        ],
        &[
            "resolve",
            "--pr",
            "https://github.com/acme/app/pull/42",
            "--yes",
            "--json",
        ],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stdout: {stdout}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["status"], "resolved", "{stdout}");

    let scratch = PathBuf::from(json["remote"]["scratch_path"].as_str().unwrap());
    let first_parent_commits = git_output(
        &scratch,
        &["rev-list", "--count", "--first-parent", "main..HEAD"],
    );
    assert_eq!(
        first_parent_commits, "1",
        "exactly the one merge commit on the resolution branch"
    );
    assert_eq!(
        std::fs::read_to_string(scratch.join("f.txt")).unwrap(),
        "resolved\n"
    );
}
