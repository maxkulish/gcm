use clap::Parser;

/// Build-stamped version: crate version plus the git short SHA from build.rs (AC-1).
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GCM_GIT_SHA"));

const EGRESS_DISCLOSURE: &str = "\
PRIVACY: gcm sends your working-tree diff and the content of untracked, non-gitignored\n\
files to the configured LLM provider (Groq) to generate the commit message. Gitignored\n\
files (e.g. .env) are never sent. See the README for each provider's data policy.";

#[derive(Parser, Debug)]
#[command(
    name = "gcm",
    version = VERSION,
    about = "Generate one signed conventional-commit from your working-tree changes via Groq.",
    after_help = EGRESS_DISCLOSURE,
    after_long_help = EGRESS_DISCLOSURE
)]
pub struct Cli {
    /// Preview the generated commit message and exit without staging or committing.
    #[arg(long)]
    pub dry_run: bool,

    /// Commit all changes as a single commit. This is the only mode in this slice
    /// (semantic grouping arrives later); accepted for forward-compatibility.
    #[arg(long)]
    pub all: bool,

    /// Auto-confirm the commit without prompting (for non-interactive / agent / CI use).
    #[arg(long, visible_alias = "no-input")]
    pub yes: bool,
}
