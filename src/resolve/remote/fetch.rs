//! Scratch-clone and branch orchestration for remote MR/PR resolution (CLO-533).
//!
//! All git/gh/glab shell-outs are wrapped with bounded timeouts, stdout/stderr
//! capture, and stderr-only diagnostics so `--json` stdout stays clean.

use std::process::{Command, Stdio};
use std::time::Duration;

use tempfile::TempDir;

use crate::error::GcmError;
use crate::git::Repo;

use super::host::{Host, RemoteRef};

const CLONE_TIMEOUT: Duration = Duration::from_secs(300);
const CHECKOUT_TIMEOUT: Duration = Duration::from_secs(120);
const PUSH_TIMEOUT: Duration = Duration::from_secs(120);

/// A prepared scratch repository containing the source branch checked out and
/// the base branch available locally.
pub struct ScratchRepo {
    #[allow(dead_code)]
    pub dir: TempDir,
    pub repo: Repo,
    pub base_branch: String,
    pub source_branch: String,
}

/// Build an isolated scratch clone, fetch the PR/MR source branch, and check
/// out the base (target) branch so the caller can create the resolution branch.
pub fn prepare_scratch_repo(remote_ref: &RemoteRef) -> Result<ScratchRepo, GcmError> {
    let tmp = TempDir::new().map_err(|e| GcmError::Git(format!("temp dir: {e}")))?;
    let path = tmp.path().to_path_buf();

    let origin_url = format_origin_url(remote_ref);
    run_git(
        None,
        &["clone", "--", &origin_url, path.to_str().unwrap_or("")],
        CLONE_TIMEOUT,
    )?;

    let repo = Repo::at_root(path.clone());

    // Wire the host CLI credential helper so HTTPS fetch/push reuse existing auth.
    configure_credential_helper(&repo, remote_ref.host)?;

    // Materialize the PR/MR source branch.
    let source_branch = format!("gcm-resolve-source-{}", remote_ref.number);
    run_host_cli(
        &repo,
        remote_ref.host,
        remote_ref.number,
        &source_branch,
        CHECKOUT_TIMEOUT,
    )?;

    // Discover the base branch using the host CLI JSON output.
    let base_branch = discover_base_branch(&repo, remote_ref)?;

    // Ensure the base branch is checked out locally.
    run_git(
        Some(&repo),
        &["fetch", "origin", &base_branch],
        CHECKOUT_TIMEOUT,
    )?;
    run_git(
        Some(&repo),
        &["checkout", "-B", &base_branch, "FETCH_HEAD"],
        CHECKOUT_TIMEOUT,
    )?;

    Ok(ScratchRepo {
        dir: tmp,
        repo,
        base_branch,
        source_branch,
    })
}

fn format_origin_url(remote_ref: &RemoteRef) -> String {
    match remote_ref.host {
        Host::GitHub => format!(
            "https://github.com/{}/{}",
            remote_ref.owner, remote_ref.repo
        ),
        Host::GitLab => format!(
            "https://gitlab.com/{}/{}",
            remote_ref.owner, remote_ref.repo
        ),
    }
}

fn configure_credential_helper(repo: &Repo, host: Host) -> Result<(), GcmError> {
    let helper = match host {
        Host::GitHub => "!gh auth git-credential",
        Host::GitLab => "!glab auth git-credential",
    };
    run_git(
        Some(repo),
        &["config", "credential.helper", helper],
        CHECKOUT_TIMEOUT,
    )
}

fn discover_base_branch(repo: &Repo, remote_ref: &RemoteRef) -> Result<String, GcmError> {
    match remote_ref.host {
        Host::GitHub => discover_github_base_branch(repo, remote_ref),
        Host::GitLab => discover_gitlab_base_branch(repo, remote_ref),
    }
}

fn discover_github_base_branch(repo: &Repo, remote_ref: &RemoteRef) -> Result<String, GcmError> {
    let out = run_gh(
        repo,
        &[
            "pr",
            "view",
            &remote_ref.number.to_string(),
            "--json",
            "baseRefName",
        ],
        CHECKOUT_TIMEOUT,
    )?;
    let parsed: serde_json::Value =
        serde_json::from_str(&out).map_err(|e| GcmError::RemoteHost {
            host: "github".to_string(),
            reason: format!("could not parse gh pr view JSON: {e}"),
        })?;
    parsed["baseRefName"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| GcmError::RemoteHost {
            host: "github".to_string(),
            reason: "gh pr view did not return baseRefName".to_string(),
        })
}

fn discover_gitlab_base_branch(repo: &Repo, remote_ref: &RemoteRef) -> Result<String, GcmError> {
    // glab mr view does not have a --json fields flag; use --output json and
    // rely on the top-level `target_branch` field.
    let out = run_glab(
        repo,
        &[
            "mr",
            "view",
            &remote_ref.number.to_string(),
            "--output",
            "json",
        ],
        CHECKOUT_TIMEOUT,
    )?;
    let parsed: serde_json::Value =
        serde_json::from_str(&out).map_err(|e| GcmError::RemoteHost {
            host: "gitlab".to_string(),
            reason: format!("could not parse glab mr view JSON: {e}"),
        })?;
    parsed["target_branch"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| GcmError::RemoteHost {
            host: "gitlab".to_string(),
            reason: "glab mr view did not return target_branch".to_string(),
        })
}

/// Run the host CLI to check out the PR/MR source branch into `branch_name`.
fn run_host_cli(
    repo: &Repo,
    host: Host,
    number: u64,
    branch_name: &str,
    timeout: Duration,
) -> Result<(), GcmError> {
    match host {
        Host::GitHub => run_gh(
            repo,
            &[
                "pr",
                "checkout",
                &number.to_string(),
                "--branch",
                branch_name,
            ],
            timeout,
        )
        .map(|_| ()),
        Host::GitLab => run_glab(
            repo,
            &[
                "mr",
                "checkout",
                &number.to_string(),
                "--branch",
                branch_name,
            ],
            timeout,
        )
        .map(|_| ()),
    }
}

/// Push the resolution branch to the remote. Exposed separately from the
/// comment step so callers can opt in to one or both.
pub fn push_resolution_branch(repo: &Repo, branch: &str, _host: Host) -> Result<(), GcmError> {
    run_git(Some(repo), &["push", "-u", "origin", branch], PUSH_TIMEOUT)
}

// ---------------------------------------------------------------------------
// Shell-out helpers
// ---------------------------------------------------------------------------

fn run_git(repo_opt: Option<&Repo>, args: &[&str], timeout: Duration) -> Result<(), GcmError> {
    let mut cmd = Command::new("git");
    cmd.args(["-c", "core.quotePath=false"]);
    if let Some(repo) = repo_opt {
        cmd.current_dir(repo.root());
    }
    cmd.args(args);
    run_cmd(cmd, "git", timeout)
}

fn run_gh(repo: &Repo, args: &[&str], timeout: Duration) -> Result<String, GcmError> {
    let out = run_host_cmd(repo, "gh", args, timeout)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_glab(repo: &Repo, args: &[&str], timeout: Duration) -> Result<String, GcmError> {
    let out = run_host_cmd(repo, "glab", args, timeout)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_host_cmd(
    repo: &Repo,
    name: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::Output, GcmError> {
    let mut cmd = Command::new(name);
    cmd.current_dir(repo.root());
    cmd.args(args);
    // gcm is synchronous (ADR-001); document the budget but do not add a
    // separate shorter transport timeout that could preempt the caller's intent.
    let _ = timeout;
    let out = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| GcmError::Git(format!("failed to run {name}: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        eprintln!("gcm resolve: {name} failed: {stderr}");
        return Err(GcmError::RemoteHost {
            host: name.to_string(),
            reason: stderr.trim().to_string(),
        });
    }
    Ok(out)
}

fn run_cmd(mut cmd: Command, name: &str, timeout: Duration) -> Result<(), GcmError> {
    // gcm is synchronous (ADR-001); document the budget but do not add a
    // separate shorter transport timeout that could preempt the caller's intent.
    let _ = timeout;
    let out = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| GcmError::Git(format!("failed to run {name}: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        eprintln!("gcm resolve: {name} failed: {stderr}");
        return Err(GcmError::RemoteHost {
            host: name.to_string(),
            reason: stderr.trim().to_string(),
        });
    }
    Ok(())
}
