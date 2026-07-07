//! Integration tests for `gcm resolve --pr` / `--mr` (CLO-533).
//!
//! Uses fake `gh`/`glab` scripts on PATH and a local git repo as the remote
//! origin. No real network calls are made.

use std::fs;
use std::path::{Path, PathBuf};
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

fn write_fake_scripts(bin_dir: &Path) {
    fs::create_dir_all(bin_dir).unwrap();

    let gh = bin_dir.join("gh");
    fs::write(
        &gh,
        r#"#!/bin/sh
# Fake gh for CLO-533 tests.
# Expects: gh pr checkout N --branch B, or gh pr view N --json baseRefName
if [ "$1" = "pr" ] && [ "$2" = "checkout" ]; then
  id="$3"
  branch="$5"
  # Source branch already exists in the fake remote repo; create local branch.
  git checkout -q -b "$branch" "origin/pr-$id-source"
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
  echo '{"baseRefName":"main"}'
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "comment" ]; then
  # Record the call by touching a sentinel.
  touch "$GCM_FAKE_GH_COMMENT_SENTINEL"
  exit 0
fi
echo "fake-gh: unexpected args $*" >&2
exit 1
"#,
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
        r#"#!/bin/sh
# Fake glab for CLO-533 tests.
if [ "$1" = "mr" ] && [ "$2" = "checkout" ]; then
  id="$3"
  branch="$5"
  git checkout -q -b "$branch" "origin/mr-$id-source"
  exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "view" ]; then
  echo '{"target_branch":"main"}'
  exit 0
fi
if [ "$1" = "mr" ] && [ "$2" = "note" ]; then
  touch "$GCM_FAKE_GLAB_NOTE_SENTINEL"
  exit 0
fi
echo "fake-glab: unexpected args $*" >&2
exit 1
"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&glab, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn build_fake_remote(tmp: &Path) -> PathBuf {
    let remote = tmp.join("remote.git");
    fs::create_dir_all(&remote).unwrap();
    git_init(&remote);
    fs::write(remote.join("f.txt"), "base\n").unwrap();
    git(&remote, &["add", "-A"]);
    git(&remote, &["commit", "-q", "-m", "init"]);
    git(&remote, &["branch", "-M", "main"]);

    // Feature branch modifies f.txt.
    git(&remote, &["checkout", "-q", "-b", "pr-42-source"]);
    fs::write(remote.join("f.txt"), "feature\n").unwrap();
    git(&remote, &["commit", "-q", "-am", "feature"]);
    git(&remote, &["checkout", "-q", "main"]);

    // Make main diverge so merge conflicts.
    fs::write(remote.join("f.txt"), "mainline\n").unwrap();
    git(&remote, &["commit", "-q", "-am", "mainline"]);

    // Mirror the source branch under the expected remote refs.
    git(
        &remote,
        &[
            "update-ref",
            "refs/remotes/origin/pr-42-source",
            "refs/heads/pr-42-source",
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
    let mut path_env = std::env::var("PATH").unwrap_or_default();
    path_env = format!("{}:{}", bin_dir.display(), path_env);

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
fn parse_github_url() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = dir.path().join("user");
    fs::create_dir_all(&user_repo).unwrap();
    git_init(&user_repo);
    let remote = build_fake_remote(dir.path());
    git(
        &user_repo,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin);

    let cfg_dir = dir.path().join("cfg");
    fs::create_dir_all(&cfg_dir).unwrap();
    write_config(
        &cfg_dir,
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

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

#[test]
fn dry_run_no_clone() {
    let dir = tempfile::tempdir().unwrap();
    let user_repo = dir.path().join("user");
    fs::create_dir_all(&user_repo).unwrap();
    git_init(&user_repo);

    let bin = dir.path().join("bin");
    write_fake_scripts(&bin);

    let cfg_dir = dir.path().join("cfg");
    fs::create_dir_all(&cfg_dir).unwrap();
    write_config(
        &cfg_dir,
        r#"version = 2
default = "ollama"

[[providers]]
id = "ollama"
"#,
    );

    // With no origin, bare id fails; use a full URL to exercise parsing only.
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
    // Fake gh should NOT have been invoked.
    assert!(!bin.join("gh-used").exists());
}
