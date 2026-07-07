//! Optional publish step for remote MR/PR resolution (CLO-533).
//!
//! Pushes the resolution branch and/or posts a summary comment on the original
//! PR/MR. Both actions are opt-in and off by default.

use std::fs;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::GcmError;
use crate::git::Repo;
use crate::resolve::report::ResolveReport;

use super::host::{Host, RemoteRef};

const COMMENT_TIMEOUT: Duration = Duration::from_secs(60);

/// What happened during the remote publish step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublishOutcome {
    pub pushed: bool,
    pub commented: bool,
}

/// Push the resolution branch and/or post a summary comment.
///
/// Kept for backward compatibility with callers that want both push and
/// comment in one call. The remote orchestrator calls `push_resolution_branch`
/// and `post_comment` directly so it can handle comment failure without aborting.
#[allow(dead_code)]
pub fn publish(
    repo: &Repo,
    remote_ref: &RemoteRef,
    resolution_branch: &str,
    report: &ResolveReport,
    push: bool,
    comment: bool,
) -> Result<PublishOutcome, GcmError> {
    let mut outcome = PublishOutcome::default();

    if push {
        super::fetch::push_resolution_branch(repo, resolution_branch, remote_ref.host)?;
        outcome.pushed = true;
    }

    if comment {
        post_comment(repo, remote_ref, report)?;
        outcome.commented = true;
    }

    Ok(outcome)
}

/// Post a summary comment on the original PR/MR. Public so the orchestrator
/// can call it directly and handle errors without aborting the resolution.
pub fn post_comment(
    repo: &Repo,
    remote_ref: &RemoteRef,
    report: &ResolveReport,
) -> Result<(), GcmError> {
    let body = build_summary_body(report);
    let tmp =
        tempfile::NamedTempFile::new().map_err(|e| GcmError::Git(format!("temp file: {e}")))?;
    fs::write(tmp.path(), &body).map_err(|e| GcmError::Git(format!("write temp file: {e}")))?;

    match remote_ref.host {
        Host::GitHub => {
            let mut cmd = Command::new("gh");
            cmd.current_dir(repo.root()).args([
                "pr",
                "comment",
                &remote_ref.number.to_string(),
                "--body-file",
                tmp.path().to_str().unwrap_or(""),
            ]);
            run_cmd_timed(cmd, "gh", COMMENT_TIMEOUT)
        }
        Host::GitLab => {
            let mut cmd = Command::new("glab");
            cmd.current_dir(repo.root()).args([
                "mr",
                "note",
                &remote_ref.number.to_string(),
                "--message",
                &body,
            ]);
            run_cmd_timed(cmd, "glab", COMMENT_TIMEOUT)
        }
    }
}

fn build_summary_body(report: &ResolveReport) -> String {
    let mut lines = vec!["gcm resolve summary".to_string()];
    let escalated: usize = report.files.iter().map(|f| f.hunks_escalated).sum();
    let total = report.files.len();
    lines.push(format!("Files: {total}, escalated hunks: {escalated}"));
    for f in &report.files {
        if f.hunks_escalated > 0 {
            lines.push(format!("- {}: {} escalated", f.path, f.hunks_escalated));
        }
    }
    lines.join("\n")
}

/// Run a command with a bounded timeout. Uses `try_wait()` polling so it is
/// cross-platform. On timeout, kills the child process and returns an error.
fn run_cmd_timed(mut cmd: Command, name: &str, timeout: Duration) -> Result<(), GcmError> {
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GcmError::Git(format!("failed to run {name}: {e}")))?;

    let deadline = Instant::now() + timeout;
    let mut child = child;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    // Drain stdout/stderr (we don't need the content here).
                    let mut stdout = child.stdout.take();
                    let mut stderr = child.stderr.take();
                    if let Some(ref mut s) = stdout {
                        let _ = s.read_to_end(&mut Vec::new());
                    }
                    if let Some(ref mut s) = stderr {
                        let _ = s.read_to_end(&mut Vec::new());
                    }
                    return Ok(());
                }
                let mut stderr_buf = String::new();
                if let Some(ref mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut stderr_buf);
                }
                eprintln!("gcm resolve: {name} failed: {stderr_buf}");
                return Err(GcmError::RemoteHost {
                    host: name.to_string(),
                    reason: stderr_buf.trim().to_string(),
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GcmError::RemoteHost {
                        host: name.to_string(),
                        reason: format!("timed out after {timeout:?}"),
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return Err(GcmError::Git(format!("failed to wait on {name}: {e}")));
            }
        }
    }
}
