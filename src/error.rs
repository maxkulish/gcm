use std::fmt;

use crate::provider::ProviderError;

/// Top-level runtime error. CLI usage errors are handled by clap (exit 2);
/// every variant here maps to exit code 1. User abort is not an error and is
/// represented as a successful `Outcome`, not a `GcmError`.
#[derive(Debug)]
pub enum GcmError {
    NotARepo,
    Git(String),
    Provider(ProviderError),
    /// Non-TTY context without `--yes`/`--no-input`: cannot prompt (ADR-001 #10).
    NonInteractive,
    Editor(String),
    EmptyMessage,
    /// The repository has unresolved merge conflicts (unmerged index entries).
    /// gcm aborts rather than risk committing conflict markers (CLO-487).
    UnmergedConflicts,
    /// `git commit` itself failed after the group was staged (e.g. a rejecting
    /// pre-commit hook, a signing failure). The group is left **staged** and the
    /// plan cache is **not** advanced so the user can fix and retry (CLO-491,
    /// FR-58). Distinct from [`GcmError::Git`] (pre-commit-step failures, which
    /// restore the index).
    CommitFailed(String),
}

impl GcmError {
    /// Whether this error means the staged group should be **left in place**.
    /// Only a commit-step failure ([`GcmError::CommitFailed`]) leaves the group
    /// staged (FR-58); every other error restores the pre-run index (FR-47).
    pub fn leaves_staged(&self) -> bool {
        matches!(self, GcmError::CommitFailed(_))
    }
}

impl fmt::Display for GcmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GcmError::NotARepo => {
                write!(f, "not a git repository (run gcm inside a git work tree)")
            }
            GcmError::Git(msg) => write!(f, "{msg}"),
            GcmError::Provider(e) => write!(f, "{e}"),
            GcmError::NonInteractive => write!(
                f,
                "no terminal available to confirm the commit. Re-run with --yes (or --no-input) \
                 to auto-confirm, or --dry-run to preview without committing."
            ),
            GcmError::Editor(msg) => write!(f, "editor failed: {msg}"),
            GcmError::EmptyMessage => write!(f, "commit message is empty; nothing committed"),
            GcmError::UnmergedConflicts => write!(
                f,
                "repository has unresolved merge conflicts; resolve them and stage your \
                 resolution before running gcm"
            ),
            GcmError::CommitFailed(msg) => write!(
                f,
                "{msg}\nThe group is left staged and the plan was not advanced; \
                 fix the issue and re-run gcm to retry this group."
            ),
        }
    }
}

impl From<ProviderError> for GcmError {
    fn from(e: ProviderError) -> Self {
        GcmError::Provider(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_commit_failed_leaves_the_group_staged() {
        // FR-58: a commit-step failure leaves the group staged; every other
        // error restores the pre-run index (FR-47).
        assert!(GcmError::CommitFailed("hook rejected".to_string()).leaves_staged());
        assert!(!GcmError::Git("git add failed".to_string()).leaves_staged());
        assert!(!GcmError::UnmergedConflicts.leaves_staged());
        assert!(!GcmError::NotARepo.leaves_staged());
    }

    #[test]
    fn commit_failed_surfaces_the_underlying_error() {
        let msg =
            GcmError::CommitFailed("git commit failed (see output above)".to_string()).to_string();
        assert!(msg.contains("git commit failed"));
        assert!(
            msg.contains("left staged"),
            "tells the user the group is kept"
        );
    }
}
