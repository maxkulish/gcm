use std::fmt;

use crate::groq::GroqError;

/// Top-level runtime error. CLI usage errors are handled by clap (exit 2);
/// every variant here maps to exit code 1. User abort is not an error and is
/// represented as a successful `Outcome`, not a `GcmError`.
#[derive(Debug)]
pub enum GcmError {
    NotARepo,
    Git(String),
    Groq(GroqError),
    /// Non-TTY context without `--yes`/`--no-input`: cannot prompt (ADR-001 #10).
    NonInteractive,
    Editor(String),
    EmptyMessage,
    /// The repository has unresolved merge conflicts (unmerged index entries).
    /// gcm aborts rather than risk committing conflict markers (CLO-487).
    UnmergedConflicts,
}

impl GcmError {
    /// Process exit code for this error. All runtime errors are 1; usage (exit 2)
    /// is produced by clap before we get here.
    pub fn exit_code(&self) -> i32 {
        1
    }
}

impl fmt::Display for GcmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GcmError::NotARepo => {
                write!(f, "not a git repository (run gcm inside a git work tree)")
            }
            GcmError::Git(msg) => write!(f, "{msg}"),
            GcmError::Groq(e) => write!(f, "{e}"),
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
        }
    }
}

impl From<GroqError> for GcmError {
    fn from(e: GroqError) -> Self {
        GcmError::Groq(e)
    }
}
