use clap::Parser;

use crate::config::AutoPolicy;
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
PROVIDER: select with --provider (groq, google, vertex, openai, anthropic, ollama) or GCM_PROVIDER\n\
(precedence flag > env > default groq); override the model with --model or the per-provider\n\
env (GCM_GROQ_MODEL / GCM_GEMINI_MODEL / GCM_VERTEX_MODEL / GCM_OPENAI_MODEL /\n\
GCM_ANTHROPIC_MODEL / GCM_OLLAMA_MODEL). Keys: GROQ_API_KEY, GEMINI_API_KEY, OPENAI_API_KEY,\n\
ANTHROPIC_API_KEY. Ollama is local and needs NO key - it talks to http://localhost:11434\n\
(override with OLLAMA_HOST / GCM_OLLAMA_BASE_URL). Vertex is keyless (Google Cloud ADC):\n\
set GCM_VERTEX_PROJECT + run `gcloud auth application-default login`, or GCM_VERTEX_TOKEN.\n\
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
    #[arg(long, global = true)]
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
    #[arg(long, global = true, visible_alias = "no-input")]
    pub yes: bool,

    ///LLM provider: groq (default), google (Gemini), vertex (Vertex AI, keyless ADC),
    /// openai, anthropic, or ollama (local, no key, zero-egress). Overrides GCM_PROVIDER
    /// (precedence: flag > env > default).
    #[arg(long, value_enum, global = true)]
    pub provider: Option<ProviderId>,

    /// Model id for the selected provider (e.g. gpt-5.6-terra).
    /// Overrides the per-provider model env var.
    #[arg(long, global = true)]
    pub model: Option<String>,

    /// Re-run the interactive provider setup wizard (updating keys/selections),
    /// then continue with the normal commit flow.
    #[arg(long)]
    pub reconfigure: bool,

    /// Optional pre-send secret scan: off (default), redact detected values, or abort
    /// before any provider request. Overrides GCM_SECRET_SCAN.
    #[arg(long, value_enum, global = true)]
    pub secret_scan: Option<SecretScanMode>,
}

/// Top-level subcommands. `gcm` with no subcommand runs the commit flow.
#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    /// Run the interactive provider setup wizard and exit.
    Config,
    /// Print active providers, models, paths, and config sources, then exit.
    Status,
    /// Interactively pick a provider, fetch and enable its models, choose a default.
    Provider,
    /// Resolve in-progress merge/rebase/cherry-pick conflicts using the LLM provider.
    Resolve {
        /// Conflict resolution temperature (overrides [conflict].temperature).
        #[arg(long)]
        conflict_temperature: Option<f64>,

        /// Validation command for resolved files (overrides [conflict].validate_cmd).
        #[arg(long)]
        conflict_validate_cmd: Option<String>,

        /// Auto-resolution policy (overrides [conflict].auto_policy).
        #[arg(long)]
        conflict_auto_policy: Option<AutoPolicy>,

        /// Glob patterns for paths that require manual review.
        #[arg(long, value_delimiter = ',')]
        conflict_sensitive_paths: Option<Vec<String>>,

        /// Skip the optional mergiraf pre-resolution stage.
        #[arg(long)]
        no_mergiraf: bool,

        /// Apply and stage confirmed resolutions but skip the finishing
        /// commit/continue (debugging escape hatch).
        #[arg(long)]
        no_finish: bool,

        /// GitHub pull request to resolve, as a full URL or numeric id.
        #[arg(long, group = "remote")]
        pr: Option<String>,

        /// GitLab merge request to resolve, as a full URL or numeric id.
        #[arg(long, group = "remote")]
        mr: Option<String>,

        /// Push the resolution branch to the remote (requires a remote resolve).
        #[arg(long, requires = "remote")]
        remote_push: bool,

        /// Post a summary comment on the PR/MR (requires a remote resolve).
        #[arg(long, requires = "remote")]
        remote_comment: bool,
    },
}

impl Cli {
    /// True if --remote-push was passed on a remote resolve.
    pub fn remote_push(&self) -> bool {
        matches!(
            &self.command,
            Some(Commands::Resolve {
                remote_push: true,
                ..
            })
        )
    }

    /// True if --remote-comment was passed on a remote resolve.
    pub fn remote_comment(&self) -> bool {
        matches!(
            &self.command,
            Some(Commands::Resolve {
                remote_comment: true,
                ..
            })
        )
    }

    /// True if --no-finish was passed on a resolve.
    pub fn no_finish(&self) -> bool {
        matches!(
            &self.command,
            Some(Commands::Resolve {
                no_finish: true,
                ..
            })
        )
    }
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

    #[test]
    fn provider_subcommand_parses() {
        let cli = Cli::try_parse_from(["gcm", "provider"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Provider)));
    }

    #[test]
    fn resolve_subcommand_parses_with_flags() {
        let cli = Cli::try_parse_from([
            "gcm",
            "resolve",
            "--conflict-temperature",
            "0.2",
            "--conflict-validate-cmd",
            "cargo check",
            "--conflict-auto-policy",
            "complex",
            "--conflict-sensitive-paths",
            "secrets/**,*.env",
            "--no-mergiraf",
            "--pr",
            "42",
            "--remote-push",
            "--remote-comment",
            "--json",
            "--dry-run",
        ])
        .unwrap();
        assert!(matches!(cli.command, Some(Commands::Resolve { .. })));
        assert!(cli.json);
        assert!(cli.dry_run);
    }

    #[test]
    fn resolve_remote_flags_are_mutually_exclusive() {
        let err = Cli::try_parse_from(["gcm", "resolve", "--pr", "42", "--mr", "7"]).unwrap_err();
        assert!(
            err.to_string().contains("remote") || err.to_string().contains("cannot"),
            "expected mutual exclusivity error, got: {err}"
        );
    }
}
