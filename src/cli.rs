use clap::Parser;

use crate::privacy::SecretScanMode;
use crate::provider::ProviderId;

/// Build-stamped version: crate version plus the git short SHA from build.rs (AC-1).
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GCM_GIT_SHA"));

const EGRESS_DISCLOSURE: &str = "\
gcm groups your working-tree changes into logical commits and commits the first group;\n\
run it again to commit the next group. Grouping operates on whole files over the entire\n\
working tree, so it overrides any manual hunk-level (git add -p) staging: group 1's files\n\
are staged in full, later groups are left unstaged (their changes are never lost).\n\
\n\
MACHINE MODE: use --json to emit a stable JSON envelope on stdout (status: plan/noop/\n\
committed/fallback/error). Combine with --plan-only for a non-destructive preview, or\n\
--yes (alias --no-input) for unattended commits. All diagnostics go to stderr.\n\
\n\
PROVIDER: select with --provider (groq, google, openai, anthropic, ollama) or GCM_PROVIDER\n\
(precedence flag > env > default groq); override the model with --model or the per-provider\n\
env (GCM_GROQ_MODEL / GCM_GEMINI_MODEL / GCM_OPENAI_MODEL / GCM_ANTHROPIC_MODEL /\n\
GCM_OLLAMA_MODEL). Keys: GROQ_API_KEY, GEMINI_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY.\n\
Ollama is local and needs NO key - it talks to http://localhost:11434 (override with\n\
OLLAMA_HOST / GCM_OLLAMA_BASE_URL).\n\
\n\
PRIVACY: gcm sends your working-tree diff and the content of untracked, non-gitignored\n\
files to the configured LLM provider to generate the plan and commit messages.\n\
Gitignored files (e.g. .env) are never sent. Repo-local .gcmignore/gcmignore patterns\n\
exclude matching paths from analysis. Use --secret-scan=redact or abort to opt into\n\
best-effort credential scanning before provider egress. With --provider=ollama and a\n\
local model, nothing leaves the machine (zero-egress); an Ollama `:cloud` model routes\n\
through Ollama Cloud and is NOT zero-egress. See the README for each provider's data policy.\n\
\n\
LOGGING: set GCM_LOG_LEVEL=off|error|warn|info|debug|trace (default off). The legacy\n\
GCM_DEBUG=1 shortcut still enables debug-level output. Logs always go to stderr.\n\
\n\
RESILIENCE: transient provider failures (HTTP 429 rate limit, 5xx) are retried with\n\
bounded exponential backoff; 400/auth errors fail fast. Set GCM_DEBUG=1 (or\n\
GCM_LOG_LEVEL=debug) to print the typed error and retry attempts to stderr.";

#[derive(Parser, Debug)]
#[command(
    name = "gcm",
    version = VERSION,
    about = "Generate one signed conventional-commit from your working-tree changes via an LLM provider.",
    after_help = EGRESS_DISCLOSURE,
    after_long_help = EGRESS_DISCLOSURE
)]
pub struct Cli {
    /// Optional subcommand. With none, gcm runs the normal commit flow.
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Preview the grouping plan (or the single-commit message with --all) and
    /// exit without staging or committing.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit a stable JSON envelope on stdout instead of human-oriented prose.
    /// All diagnostics are sent to stderr so stdout contains a single valid
    /// JSON object. Global so it is accepted after a subcommand too
    /// (e.g. `gcm status --json`).
    #[arg(long, global = true)]
    pub json: bool,

    /// Generate the plan (or single-commit preview with --all) and exit without
    /// staging, committing, or touching the cache.
    #[arg(long)]
    pub plan_only: bool,

    /// Skip grouping and commit all changes as a single commit.
    #[arg(long)]
    pub all: bool,

    /// Discard any cached grouping plan and re-analyze from scratch.
    #[arg(long)]
    pub reset: bool,

    /// Auto-confirm the commit without prompting (for non-interactive / agent / CI use).
    #[arg(long, visible_alias = "no-input")]
    pub yes: bool,

    ///LLM provider: groq (default), google (Gemini), openai, anthropic, or ollama (local,
    /// no key, zero-egress). Overrides GCM_PROVIDER (precedence: flag > env > default).
    #[arg(long, value_enum)]
    pub provider: Option<ProviderId>,

    /// Model id for the selected provider (e.g. gpt-4o-mini-2024-07-18).
    /// Overrides the per-provider model env var.
    #[arg(long)]
    pub model: Option<String>,

    /// Re-run the interactive provider setup wizard (updating keys/selections),
    /// then continue with the normal commit flow.
    #[arg(long)]
    pub reconfigure: bool,

    /// Optional pre-send secret scan: off (default), redact detected values, or abort
    /// before any provider request. Overrides GCM_SECRET_SCAN.
    #[arg(long, value_enum)]
    pub secret_scan: Option<SecretScanMode>,
}

/// Top-level subcommands. `gcm` with no subcommand runs the commit flow.
#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    /// Run the interactive provider setup wizard and exit.
    Config,
    /// Print active providers, models, paths, and config sources, then exit.
    Status,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        // catches subcommand/flag conflicts at test time
        Cli::command().debug_assert();
    }

    #[test]
    fn no_subcommand_parses_to_commit_flow() {
        let cli = Cli::try_parse_from(["gcm"]).unwrap();
        assert!(cli.command.is_none(), "no subcommand -> commit flow");
        assert!(!cli.reconfigure);
        // existing flags still parse alongside the optional subcommand
        let cli = Cli::try_parse_from(["gcm", "--dry-run", "--provider", "ollama"]).unwrap();
        assert!(cli.command.is_none());
        assert!(cli.dry_run);
    }

    #[test]
    fn config_subcommand_parses() {
        let cli = Cli::try_parse_from(["gcm", "config"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Config)));
    }

    #[test]
    fn status_subcommand_parses() {
        let cli = Cli::try_parse_from(["gcm", "status"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Status)));
    }

    #[test]
    fn json_is_global_after_subcommand() {
        // --json is global, so it parses both before and after the subcommand.
        let cli = Cli::try_parse_from(["gcm", "status", "--json"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Status)));
        assert!(cli.json);
        let cli = Cli::try_parse_from(["gcm", "--json", "status"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Status)));
        assert!(cli.json);
    }

    #[test]
    fn reconfigure_flag_parses() {
        let cli = Cli::try_parse_from(["gcm", "--reconfigure"]).unwrap();
        assert!(cli.reconfigure);
        assert!(cli.command.is_none());
    }
}
