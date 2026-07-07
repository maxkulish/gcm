//! Validation gate for proposed conflict resolutions (CLO-531, ST3).
//!
//! The default check is syntax-safe: no conflict markers remain. An optional
//! user-configured `validate_cmd` is run on a temp copy rooted at the repo so
//! commands like `cargo check` or `node --check` behave naturally.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::git::Repo;

use super::markers::has_conflict_markers;

/// Validate a proposed resolution. Returns `Ok(())` if it passes.
pub fn validate(
    resolved_text: &str,
    validate_cmd: Option<&str>,
    repo: &Repo,
    path: &str,
) -> Result<(), ValidationError> {
    if has_conflict_markers(resolved_text) {
        return Err(ValidationError::ConflictMarkers);
    }
    if let Some(cmd) = validate_cmd {
        run_validate_cmd(cmd, resolved_text, repo, path)?;
    }
    Ok(())
}

#[derive(Debug)]
pub enum ValidationError {
    ConflictMarkers,
    #[allow(dead_code)]
    ValidateCmdFailed {
        stdout: String,
        stderr: String,
    },
}

fn run_validate_cmd(
    cmd: &str,
    resolved_text: &str,
    repo: &Repo,
    _path: &str,
) -> Result<(), ValidationError> {
    let mut tmp = tempfile::Builder::new()
        .prefix("gcm-resolve-")
        .suffix("-validate")
        .tempfile()
        .map_err(|e| ValidationError::ValidateCmdFailed {
            stdout: String::new(),
            stderr: format!("could not create temp file: {e}"),
        })?;
    tmp.write_all(resolved_text.as_bytes())
        .map_err(|e| ValidationError::ValidateCmdFailed {
            stdout: String::new(),
            stderr: format!("could not write temp file: {e}"),
        })?;
    tmp.flush().ok();

    let output = Command::new("sh")
        .current_dir(repo.root())
        .arg("-c")
        .arg(format!("{} \"{}\"", cmd, tmp.path().display()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| ValidationError::ValidateCmdFailed {
            stdout: String::new(),
            stderr: format!("could not run validate_cmd '{cmd}': {e}"),
        })?;

    if output.status.success() {
        return Ok(());
    }

    Err(ValidationError::ValidateCmdFailed {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Repo;

    fn repo() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        std::process::Command::new("git")
            .current_dir(&root)
            .args(["init", "-q"])
            .output()
            .expect("git init");
        (dir, Repo::at_root(root))
    }

    #[test]
    fn rejects_conflict_markers() {
        let (_dir, repo) = repo();
        assert!(matches!(
            validate("<<<<<<< HEAD\n", None, &repo, "x"),
            Err(ValidationError::ConflictMarkers)
        ));
    }

    #[test]
    fn accepts_clean_text() {
        let (_dir, repo) = repo();
        assert!(validate("no conflicts\n", None, &repo, "x").is_ok());
    }

    #[test]
    fn validate_cmd_success() {
        let (_dir, repo) = repo();
        let result = validate("ok\n", Some("true"), &repo, "x");
        assert!(result.is_ok(), "expected ok, got {result:?}");
    }

    #[test]
    fn validate_cmd_failure() {
        let (_dir, repo) = repo();
        assert!(matches!(
            validate("ok\n", Some("false"), &repo, "x"),
            Err(ValidationError::ValidateCmdFailed { .. })
        ));
    }
}
