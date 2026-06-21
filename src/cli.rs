use clap::Parser;

use crate::provider::ProviderId;

/// Build-stamped version: crate version plus the git short SHA from build.rs (AC-1).
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GCM_GIT_SHA"));

const EGRESS_DISCLOSURE: &str = "\
gcm groups your working-tree changes into logical commits and commits the first group;\n\
run it again to commit the next group. Grouping operates on whole files over the entire\n\
working tree, so it overrides any manual hunk-level (git add -p) staging: group 1's files\n\
are staged in full, later groups are left unstaged (their changes are never lost).\n\
\n\
PROVIDER: select with --provider (groq, google, openai) or GCM_PROVIDER (precedence\n\
flag > env > default groq); override the model with --model or the per-provider env\n\
(GCM_GROQ_MODEL / GCM_GEMINI_MODEL / GCM_OPENAI_MODEL). Keys: GROQ_API_KEY,\n\
GEMINI_API_KEY, OPENAI_API_KEY.\n\
\n\
PRIVACY: gcm sends your working-tree diff and the content of untracked, non-gitignored\n\
files to the configured LLM provider to generate the plan and commit messages.\n\
Gitignored files (e.g. .env) are never sent. See the README for each provider's data policy.\n\
\n\
RESILIENCE: transient provider failures (HTTP 429 rate limit, 5xx) are retried with\n\
bounded exponential backoff; 400/auth errors fail fast. Set GCM_DEBUG=1 to print the\n\
typed error and retry attempts to stderr.";

#[derive(Parser, Debug)]
#[command(
    name = "gcm",
    version = VERSION,
    about = "Generate one signed conventional-commit from your working-tree changes via an LLM provider.",
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

    /// Discard any cached grouping plan and re-analyze from scratch.
    #[arg(long)]
    pub reset: bool,

    /// Auto-confirm the commit without prompting (for non-interactive / agent / CI use).
    #[arg(long, visible_alias = "no-input")]
    pub yes: bool,

    /// LLM provider: groq (default), google (Gemini), or openai. Overrides
    /// GCM_PROVIDER (precedence: flag > env > default).
    #[arg(long, value_enum)]
    pub provider: Option<ProviderId>,

    /// Model id for the selected provider (e.g. gpt-4o-mini-2024-07-18).
    /// Overrides the per-provider model env var.
    #[arg(long)]
    pub model: Option<String>,
}
