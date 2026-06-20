use clap::Parser;

/// Build-stamped version: crate version plus the git short SHA from build.rs (AC-1).
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GCM_GIT_SHA"));

const EGRESS_DISCLOSURE: &str = "\
gcm groups your working-tree changes into logical commits and commits the first group;\n\
run it again to commit the next group. Grouping operates on whole files over the entire\n\
working tree, so it overrides any manual hunk-level (git add -p) staging: group 1's files\n\
are staged in full, later groups are left unstaged (their changes are never lost).\n\
\n\
PRIVACY: gcm sends your working-tree diff and the content of untracked, non-gitignored\n\
files to the configured LLM provider (Groq) to generate the plan and commit messages.\n\
Gitignored files (e.g. .env) are never sent. See the README for each provider's data policy.";

#[derive(Parser, Debug)]
#[command(
    name = "gcm",
    version = VERSION,
    about = "Generate one signed conventional-commit from your working-tree changes via Groq.",
    after_help = EGRESS_DISCLOSURE,
    after_long_help = EGRESS_DISCLOSURE
)]
pub struct Cli {
    /// Preview the grouping plan (or the single-commit message with --all) and
    /// exit without staging or committing.
    #[arg(long)]
    pub dry_run: bool,

    /// Skip grouping and commit all changes as a single commit.
    #[arg(long)]
    pub all: bool,

    /// Auto-confirm the commit without prompting (for non-interactive / agent / CI use).
    #[arg(long, visible_alias = "no-input")]
    pub yes: bool,
}
