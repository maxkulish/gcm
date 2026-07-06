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
    /// First-run setup is needed but there is no terminal to run the wizard
    /// (CLO-496). The caller prints `config::non_tty_instructions()` to stderr
    /// and exits non-zero; it occurs before any staging.
    OnboardingRequired,
    /// User/configuration input outside provider selection (provider config
    /// errors are represented by `ProviderError::Config`).
    Config(String),
    /// The optional pre-send secret scan found credential-looking content and
    /// was configured to abort before provider egress (CLO-490, FR-50).
    SecretDetected {
        count: usize,
    },
    /// `gcm resolve` was called but no merge/rebase/cherry-pick is in progress.
    NoConflictInProgress,
    /// `gcm resolve` was called but no unmerged files were found.
    NoConflicts,
    /// A conflict resolution failed validation and was left conflicted for human review.
    ResolutionEscalated { path: String, reason: String },
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
            GcmError::OnboardingRequired => write!(
                f,
                "no provider is configured and there is no terminal to run setup. \
                 Run `gcm config` interactively, or export a provider key (e.g. \
                 GROQ_API_KEY) and set GCM_PROVIDER, then retry."
            ),
            GcmError::Config(msg) => write!(f, "{msg}"),
            GcmError::SecretDetected { count } => write!(
                f,
                "secret scan detected {count} credential-like value(s); no provider request was sent. \
                 Remove the secret, add the path to .gcmignore, or re-run with --secret-scan=redact."
            ),
            GcmError::NoConflictInProgress => write!(
                f,
                "no merge, rebase, or cherry-pick is in progress; run `git merge`, `git rebase`, or `git cherry-pick` first."
            ),
            GcmError::NoConflicts => write!(
                f,
                "merge/rebase/cherry-pick in progress, but no unmerged files remain."
            ),
            GcmError::ResolutionEscalated { path, reason } => write!(
                f,
                "resolution for {path} failed validation: {reason}. The file is left conflicted for manual resolution."
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
        // CLO-496: onboarding-required occurs before staging, so nothing is kept.
        assert!(!GcmError::OnboardingRequired.leaves_staged());
        assert!(!GcmError::SecretDetected { count: 1 }.leaves_staged());
        assert!(!GcmError::NoConflictInProgress.leaves_staged());
        assert!(!GcmError::NoConflicts.leaves_staged());
        assert!(!GcmError::ResolutionEscalated { path: "x".to_string(), reason: "r".to_string() }.leaves_staged());
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

    #[test]
    fn secret_detected_mentions_no_provider_request() {
        let msg = GcmError::SecretDetected { count: 2 }.to_string();
        assert!(msg.contains("no provider request was sent"));
        assert!(msg.contains("--secret-scan=redact"));
    }

    #[test]
    fn new_resolve_error_messages_are_actionable() {
        let msg = GcmError::NoConflictInProgress.to_string();
        assert!(msg.contains("merge"));
        assert!(msg.contains("rebase"));
        let msg = GcmError::NoConflicts.to_string();
        assert!(msg.contains("no unmerged files"));
        let msg = GcmError::ResolutionEscalated { path: "src/lib.rs".to_string(), reason: "validation failed".to_string() }.to_string();
        assert!(msg.contains("src/lib.rs"));
        assert!(msg.contains("validation failed"));
    }
}
