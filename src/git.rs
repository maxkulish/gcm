use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::GcmError;

/// Thin typed wrapper over the `git` binary (ADR-001 #1). All path-reading
/// commands pass `-c core.quotePath=false` and operate from the repo root so
/// porcelain/diff paths and filesystem paths agree.
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// Discover the enclosing work tree. `Ok(None)` when CWD is not inside a git
    /// repository; `Err` only when the `git` binary itself cannot be run.
    pub fn discover() -> Result<Option<Repo>, GcmError> {
        let inside = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git: {e}")))?;
        if !inside.status.success() || String::from_utf8_lossy(&inside.stdout).trim() != "true" {
            return Ok(None);
        }
        let top = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git: {e}")))?;
        if !top.status.success() {
            return Ok(None);
        }
        let root = String::from_utf8_lossy(&top.stdout).trim().to_string();
        Ok(Some(Repo {
            root: PathBuf::from(root),
        }))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A `git` Command rooted at the repo with quotePath disabled.
    fn git(&self, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        c.current_dir(&self.root);
        c.args(["-c", "core.quotePath=false"]);
        c.args(args);
        c
    }

    /// Run a git command, capturing stdout as a (lossy) UTF-8 string.
    fn capture(&self, args: &[&str]) -> Result<String, GcmError> {
        let out = self
            .git(args)
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", args.join(" "))))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Whether HEAD resolves (false on an unborn branch / fresh repo).
    pub fn has_head(&self) -> bool {
        self.git(&["rev-parse", "--verify", "--quiet", "HEAD"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// True if there are any uncommitted changes: unstaged, staged, or untracked
    /// (gitignore-respecting). Drives the "no changes -> exit 0" path (FR-9).
    pub fn has_changes(&self) -> Result<bool, GcmError> {
        let unstaged = !self.quiet_diff(&["diff", "--quiet"])?;
        let staged = !self.quiet_diff(&["diff", "--cached", "--quiet"])?;
        let untracked = !self.untracked_files()?.is_empty();
        Ok(unstaged || staged || untracked)
    }

    /// Run a `--quiet` diff; returns true when there is NO difference (exit 0).
    fn quiet_diff(&self, args: &[&str]) -> Result<bool, GcmError> {
        let status = self
            .git(args)
            .status()
            .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", args.join(" "))))?;
        Ok(status.success())
    }

    /// Diff stat for the prompt header. On an unborn branch (no HEAD) git
    /// synthesizes the empty tree for `--cached`, so no empty-tree object is
    /// required (AC-14); otherwise diff the working tree against HEAD.
    pub fn diff_stat(&self) -> Result<String, GcmError> {
        if self.has_head() {
            self.capture(&["diff", "--stat", "HEAD"])
        } else {
            self.capture(&["diff", "--cached", "--stat"])
        }
    }

    /// Full diff (no color) for the prompt body. HEAD when present, else the
    /// staged-vs-empty diff on an unborn branch (untracked files are gathered
    /// separately, so this covers all tracked changes).
    pub fn diff_full(&self) -> Result<String, GcmError> {
        if self.has_head() {
            self.capture(&["diff", "--no-color", "HEAD"])
        } else {
            self.capture(&["diff", "--no-color", "--cached"])
        }
    }

    /// Untracked files honoring gitignore (`--exclude-standard`), NUL-split so
    /// unicode/space/newline paths survive (FR-31, FR-48).
    pub fn untracked_files(&self) -> Result<Vec<String>, GcmError> {
        let out = self
            .git(&["ls-files", "--others", "--exclude-standard", "-z"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git ls-files: {e}")))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git ls-files failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(out
            .stdout
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect())
    }

    /// Capture the current index as a tree object (FR-47 transaction start).
    pub fn snapshot_index(&self) -> Result<String, GcmError> {
        Ok(self.capture(&["write-tree"])?.trim().to_string())
    }

    /// Restore the index to a previously-snapshotted tree. The working tree is
    /// untouched; this only rewinds staging (FR-47 restore on abort/failure).
    pub fn restore_index(&self, tree: &str) -> Result<(), GcmError> {
        self.capture(&["read-tree", tree]).map(|_| ())
    }

    /// Stage every change (the tracer commits all changes as one commit, FR-6).
    pub fn stage_all(&self) -> Result<(), GcmError> {
        self.capture(&["add", "-A"]).map(|_| ())
    }

    /// Create a signed commit (FR-4). Stdio is inherited so GPG/SSH passphrase
    /// (pinentry) prompts work on the user's terminal.
    pub fn commit_signed(&self, message: &str) -> Result<(), GcmError> {
        let status = self
            .git(&["commit", "-S", "-m", message])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| GcmError::Git(format!("failed to run git commit: {e}")))?;
        if !status.success() {
            return Err(GcmError::Git(
                "git commit failed (see output above); index restored".to_string(),
            ));
        }
        Ok(())
    }
}
