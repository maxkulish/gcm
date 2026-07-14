mod cache;
mod cli;
mod config;
mod debug;
mod diff;
mod error;
mod git;
mod output;
mod paths;
mod plan;
mod privacy;
mod provider;
mod resolve;
mod status;
mod ui;

use std::collections::HashSet;
use std::io::IsTerminal;

use clap::Parser;

use cli::{Cli, Commands};
use error::GcmError;
use git::{ChangedFile, Repo};
use output::Envelope;
use plan::Plan;
use privacy::Privacy;
use provider::{ErrorKind, Provider};
use ui::Decision;

fn main() {
    let args = Cli::parse();
    std::process::exit(run(&args));
}

/// Returns the process exit code: 0 = success/noop/abort, 1 = runtime error
/// (usage errors exit 2 via clap before we get here). See FR-9, FR-39.
fn run(args: &Cli) -> i32 {
    // The `config` subcommand is a standalone interactive action (CLO-496): run
    // the wizard, persist, and exit without touching the repo or the commit flow.
    if matches!(args.command, Some(Commands::Config)) {
        return run_config_subcommand();
    }

    // The `status` subcommand is read-only introspection (CLO-515): print active
    // providers/models/paths/sources and exit, before any repo read, onboarding,
    // env hydration, or LLM call.
    if matches!(args.command, Some(Commands::Status)) {
        return status::run_status_subcommand(args);
    }

    // The `provider` subcommand is a standalone interactive action (CLO-516): run
    // the cliclack provider/model wizard, persist, and exit without touching the
    // repo or the commit flow.
    if matches!(args.command, Some(Commands::Provider)) {
        return run_provider_subcommand();
    }

    // The `resolve` subcommand is an LLM-assisted conflict resolver (CLO-531).
    if matches!(args.command, Some(Commands::Resolve { .. })) {
        return run_resolve_subcommand(args);
    }

    let env = execute(args);
    let is_error = env.status == output::STATUS_ERROR;

    if args.json {
        output::emit(&env);
    } else {
        print_human(&env);
    }

    if is_error {
        1
    } else {
        0
    }
}

/// Run the `gcm resolve` subcommand (CLO-531). Delegates to the resolve module
/// and prints the outcome envelope.
fn run_resolve_subcommand(args: &Cli) -> i32 {
    // Check if this is a remote resolve (--pr or --mr).
    let is_remote = matches!(
        args.command,
        Some(Commands::Resolve { pr: Some(_), .. }) | Some(Commands::Resolve { mr: Some(_), .. })
    );

    // For remote dry-run with a full URL, we don't need a local repo.
    if is_remote && args.dry_run {
        let remote_arg = extract_remote_arg_from_cli(args).unwrap_or_default();
        if remote_arg.starts_with("http://")
            || remote_arg.starts_with("https://")
            || remote_arg.starts_with("git@")
        {
            return run_remote_resolve(None, args);
        }
    }

    let repo = match Repo::discover() {
        Ok(Some(r)) => r,
        Ok(None) => {
            if is_remote {
                // Remote non-dry-run or bare-id dry-run needs a repo.
                return run_remote_resolve(None, args);
            }
            if args.json {
                output::emit(&output::error(
                    None,
                    None,
                    Some(output::MODE_DRY_RUN),
                    &GcmError::NotARepo,
                ));
            } else {
                eprintln!("gcm: {}", GcmError::NotARepo);
            }
            return 1;
        }
        Err(e) => {
            if args.json {
                output::emit(&output::error(None, None, Some(output::MODE_DRY_RUN), &e));
            } else {
                eprintln!("gcm: {e}");
            }
            return 1;
        }
    };

    if is_remote {
        return run_remote_resolve(Some(&repo), args);
    }

    match resolve::run_resolve(args) {
        Ok(()) => {
            if args.json {
                // run_resolve already emitted the JSON envelope.
            }
            0
        }
        Err(e) => {
            if args.json {
                output::emit(&output::error(None, None, Some(output::MODE_DRY_RUN), &e));
            } else {
                eprintln!("gcm: {e}");
            }
            1
        }
    }
}

/// Extract the remote argument string from CLI args.
fn extract_remote_arg_from_cli(args: &Cli) -> Option<String> {
    match &args.command {
        Some(Commands::Resolve { pr: Some(p), .. }) => Some(p.clone()),
        Some(Commands::Resolve { mr: Some(m), .. }) => Some(m.clone()),
        _ => None,
    }
}

/// Run the remote resolve path and print results.
fn run_remote_resolve(repo: Option<&Repo>, args: &Cli) -> i32 {
    match resolve::run_resolve_remote_opt(repo, args) {
        Ok(report) => {
            if args.json {
                resolve::report::emit(&report);
            } else if let Some(ref remote) = report.remote {
                // Human-readable summary for remote runs.
                eprintln!(
                    "gcm resolve: {} #{} -> branch {}",
                    remote.host.cli_name(),
                    remote.number,
                    remote.resolution_branch
                );
                eprintln!(
                    "  base: {}, source: {}",
                    remote.base_branch, remote.source_branch
                );
                if remote.pushed {
                    eprintln!("  pushed: yes");
                }
                if remote.commented {
                    eprintln!("  commented: yes");
                }
                eprintln!("  status: {}", report.status_label());
                if let Some(ref path) = remote.scratch_path {
                    eprintln!("  scratch: {path}");
                }
            }
            0
        }
        Err(e) => {
            if args.json {
                output::emit(&output::error(None, None, Some(output::MODE_DRY_RUN), &e));
            } else {
                eprintln!("gcm: {e}");
            }
            1
        }
    }
}

/// Run the `gcm config` subcommand: launch the wizard, persist the result, and
/// return the process exit code (0 on success, 1 on failure). Interactive; all
/// output goes to stderr.
fn run_config_subcommand() -> i32 {
    // The wizard is interactive; without a terminal it cannot prompt. Fail fast
    // with guidance instead of erroring on the first EOF read.
    if !std::io::stdin().is_terminal() {
        eprintln!("gcm: `gcm config` needs an interactive terminal to run the setup wizard.");
        eprintln!("{}", config::non_tty_instructions());
        return 1;
    }
    match config::run_wizard() {
        Ok(cfg) => {
            if let Err(e) = config::save(&cfg) {
                eprintln!("gcm: error: could not save configuration: {e}");
                return 1;
            }
            match config::config_path() {
                Some(p) => eprintln!("gcm: configuration saved to {}", p.display()),
                None => eprintln!("gcm: configuration saved"),
            }
            0
        }
        Err(e) => {
            eprintln!("gcm: {e}");
            1
        }
    }
}

/// Run the `gcm provider` subcommand (CLO-516): launch the cliclack wizard and
/// return the process exit code (0 saved, 1 cancelled or failed). Interactive, so
/// without a terminal it fails fast with guidance rather than erroring on the first
/// `/dev/tty` read.
fn run_provider_subcommand() -> i32 {
    if !std::io::stdin().is_terminal() {
        eprintln!("gcm: `gcm provider` needs an interactive terminal to run the wizard.");
        eprintln!("{}", config::non_tty_instructions());
        return 1;
    }
    match config::run_provider_wizard() {
        Ok(true) => 0,
        Ok(false) => 1, // user cancelled; the wizard already printed the cancel notice
        Err(e) => {
            eprintln!("gcm: {e}");
            1
        }
    }
}

/// Ensure a provider is configured before the commit flow (CLO-496). With
/// `--reconfigure`, always re-run the wizard. Otherwise: hydrate an existing
/// config into the environment, or - on an unconfigured first run - launch the
/// wizard in a TTY, or return [`GcmError::OnboardingRequired`] when there is no
/// terminal. An env-configured user (key/`GCM_PROVIDER`/`--provider`) is left
/// untouched, preserving `flag > env > config > default` precedence.
fn ensure_configured(args: &Cli) -> Result<(), GcmError> {
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();

    if args.reconfigure {
        if !interactive {
            return Err(GcmError::OnboardingRequired);
        }
        let cfg = config::run_wizard()?;
        save_config_best_effort(&cfg);
        config::apply_to_env(&cfg);
        return enforce_enabled_model(&cfg, args);
    }

    if let Some(cfg) = config::load() {
        config::apply_to_env(&cfg);
        return enforce_enabled_model(&cfg, args);
    }

    if !config::needs_onboarding(args.provider) {
        // Env-configured (or flag-driven): proceed without interruption. No config
        // is loaded, so there is no enabled-set to enforce (unrestricted).
        return Ok(());
    }

    if interactive {
        let cfg = config::run_wizard()?;
        save_config_best_effort(&cfg);
        config::apply_to_env(&cfg);
        enforce_enabled_model(&cfg, args)
    } else {
        Err(GcmError::OnboardingRequired)
    }
}

/// Reject a resolved model outside the selected provider's enabled set (CLO-516).
/// A no-op when the provider has an empty/absent `models` (unrestricted). Runs
/// after `apply_to_env`, so it resolves the exact model `provider::select` will
/// use (the config `model` is already bridged into the env the resolver reads):
/// precedence stays `--model` flag > per-provider env > config > default.
fn enforce_enabled_model(cfg: &config::Config, args: &Cli) -> Result<(), GcmError> {
    let env = std::env::var("GCM_PROVIDER").ok();
    let id =
        provider::pick_provider_id(args.provider, env.as_deref()).map_err(GcmError::Provider)?;
    let (model, _src) =
        provider::resolve_model_with_source(id, args.model.as_deref(), |v| std::env::var(v).ok());
    config::model_is_enabled(cfg, id, &model).map_err(GcmError::Config)
}

/// Persist the config, warning (not failing the run) if the write fails - the
/// in-memory config is still hydrated into the environment for this run.
fn save_config_best_effort(cfg: &config::Config) {
    if let Err(e) = config::save(cfg) {
        eprintln!("gcm: warning: could not save configuration: {e}");
    }
}

/// Execute the requested operation and return the single machine-readable
/// envelope that describes the outcome. All stderr diagnostics are emitted
/// along the way; stdout is only touched via the returned envelope (or via
/// the interactive confirmation prompt in plain mode).
fn execute(args: &Cli) -> Envelope {
    let repo = match Repo::discover() {
        Ok(Some(r)) => r,
        Ok(None) => {
            return output::error(None, None, Some(mode_from_args(args)), &GcmError::NotARepo);
        }
        Err(e) => return output::error(None, None, Some(mode_from_args(args)), &e),
    };

    // `--reset` discards any cached plan up front (FR-8/FR-28), before the
    // no-changes check so it clears even when the tree is currently clean.
    if args.reset {
        cache::clear(&repo);
    }

    // First-run onboarding (CLO-496): load + hydrate config, or launch the wizard
    // on an unconfigured first run. Runs before the no-changes and non-TTY guards
    // so an unconfigured non-TTY user gets actionable setup instructions (and a
    // JSON error envelope in `--json` mode) rather than a generic NonInteractive
    // error. An env-configured user is never interrupted.
    if let Err(e) = ensure_configured(args) {
        if matches!(e, GcmError::OnboardingRequired) {
            eprintln!("{}", config::non_tty_instructions());
        }
        return output::error(None, None, Some(mode_from_args(args)), &e);
    }

    if let Err(e) = repo.has_changes() {
        return output::error(None, None, Some(mode_from_args(args)), &e);
    }
    if !repo.has_changes().unwrap_or(false) {
        return output::noop(None, None, noop_mode(args));
    }

    // Fail fast before sending any diff to the provider if we could not confirm
    // the commit anyway (ADR-001 #10, AC-11). `--plan-only` is non-interactive
    // safe: it never prompts and never mutates the index.
    if ui::needs_terminal_but_absent(args.yes, args.dry_run || args.plan_only) {
        return output::error(
            None,
            None,
            Some(mode_from_args(args)),
            &GcmError::NonInteractive,
        );
    }

    // Merge-state guard (CLO-487 review-2 #2) runs BEFORE any grouping bypass,
    // including `--all`: staging a conflicted working tree on *either* path
    // (grouping `add` or single-commit `add -A`) would bake `<<<<<<<` markers
    // into the commit, so an unresolved conflict must abort regardless of flags.
    let raw_changed = match repo.changed_files() {
        Ok(c) => c,
        Err(e) => return output::error(None, None, Some(mode_from_args(args)), &e),
    };
    if raw_changed.iter().any(|c| c.is_unmerged()) {
        return output::error(
            None,
            None,
            Some(mode_from_args(args)),
            &GcmError::UnmergedConflicts,
        );
    }

    let privacy = match Privacy::load(&repo, args.secret_scan) {
        Ok(p) => p,
        Err(e) => return output::error(None, None, Some(mode_from_args(args)), &e),
    };
    let changed = privacy.filter_changed(&raw_changed);
    if changed.is_empty() {
        if !raw_changed.is_empty() {
            eprintln!("gcm: all changed paths are excluded by .gcmignore/gcmignore");
        }
        return output::noop(None, None, noop_mode(args));
    }

    // Select the provider once (FR-12, precedence flag > env > default). Pure
    // w.r.t. the API key (keys are read lazily inside the calls), so this runs
    // before the no-changes/merge branches without needing a key. An unknown
    // provider name fails fast here.
    let provider = match provider::select(args.provider, args.model.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            return output::error(
                None,
                None,
                Some(mode_from_args(args)),
                &GcmError::Provider(e),
            )
        }
    };
    let provider_name = provider.name().to_string();
    let model_id = provider.cache_model_id();
    crate::debug_log!("provider: {} ({})", provider_name, model_id);

    // FR-46: warn before resetting a user-curated index. Both real commit paths
    // discard the user's staging (grouping resets via `clear_staged`; `--all`
    // overrides via `git add -A`), so a pre-existing curated/partial index is
    // about to be lost. Gate on "will the index actually be mutated": skip for
    // `--dry-run` and `--plan-only` (both are no-mutation).
    if !args.dry_run && !args.plan_only {
        let staged = raw_changed.iter().filter(|c| c.is_staged()).count();
        if staged > 0 {
            let partial = raw_changed
                .iter()
                .filter(|c| c.is_partially_staged())
                .count();
            eprintln!("{}", ui::curated_index_warning(staged, partial));
        }
    }

    // `--all`, or a clean merge-in-progress, bypasses grouping and commits
    // everything as one. A clean `MERGE_HEAD` makes `git commit` finalize the
    // merge as a proper two-parent merge commit. The single-commit path clears
    // the cached plan (FR-28).
    if args.all || repo.is_merging() {
        return single_commit_path(
            &repo,
            args,
            provider.as_ref(),
            &changed,
            &privacy,
            provider_name.as_str(),
            model_id.as_str(),
        );
    }

    // Grouping path. A fresh plan is persisted to the per-repo cache; a cache
    // hit reuses it and skips the grouping call entirely (FR-25/FR-2). The
    // provider-qualified model is folded into the freshness fingerprint
    // (FR-27), so a provider/model switch re-analyzes. A structured-output/parse/
    // validation failure falls back to the single-commit path with an announced
    // reason (never silent); a fatal error (missing key, git failure) is
    // returned as an error envelope.
    let (plan, cached) = match cache::load(&repo, &changed, &model_id) {
        Some(plan) => {
            let change_set: HashSet<String> = changed.iter().map(|c| c.path.clone()).collect();
            if let Err(reason) = plan::validate_cached(&plan, &change_set) {
                let raw_code = output::fallback_raw_code(&reason.to_string());
                let msg = format!("cached plan invalid ({reason})");
                crate::debug_log!("{}; falling back to single-commit", msg);
                return run_fallback(
                    &repo,
                    args,
                    provider.as_ref(),
                    &changed,
                    &privacy,
                    provider_name.as_str(),
                    model_id.as_str(),
                    msg,
                    raw_code,
                );
            }
            (plan, true)
        }
        None => match build_plan(&repo, &changed, provider.as_ref(), &privacy) {
            Ok(plan) => {
                if !args.plan_only {
                    // `--dry-run` uses/saves but does not advance (FR-7); `--yes`
                    // and the default interactive path also save.
                    cache::save(&repo, &plan, &changed, &model_id);
                }
                (plan, false)
            }
            Err(BuildError::Fatal(e)) => {
                return output::error(
                    Some(provider_name.as_str()),
                    Some(model_id.as_str()),
                    grouped_mode(args),
                    &e,
                );
            }
            Err(BuildError::Fallback { reason, raw_code }) => {
                crate::debug_log!("{}; falling back to single-commit", reason);
                return run_fallback(
                    &repo,
                    args,
                    provider.as_ref(),
                    &changed,
                    &privacy,
                    provider_name.as_str(),
                    model_id.as_str(),
                    reason,
                    raw_code,
                );
            }
        },
    };

    commit_first_group(
        &repo,
        args,
        &changed,
        &plan,
        cached,
        model_id.as_str(),
        provider.as_ref(),
        provider_name.as_str(),
        &privacy,
    )
}

/// Whether the group-commit flow committed or the user aborted. Gates cache
/// advancement: only a real commit advances the plan (FR-26) - never an abort.
#[derive(Debug, PartialEq, Eq)]
enum CommitOutcome {
    Committed,
    Aborted,
}

/// Outcome of a failed grouping attempt: `Fatal` errors abort (the single-commit
/// path needs the same resource), `Fallback` errors degrade to single-commit.
enum BuildError {
    Fatal(GcmError),
    Fallback { reason: String, raw_code: String },
}

/// Gather the grouping context, request the plan, and basic-validate it.
/// Model/plan failures (structured-output error, unparseable JSON, empty
/// response, validation) are `Fallback`; a missing key or git failure is
/// `Fatal`.
fn build_plan(
    repo: &Repo,
    changed: &[ChangedFile],
    provider: &dyn Provider,
    privacy: &Privacy,
) -> Result<Plan, BuildError> {
    let ctx = diff::gather_for_grouping(repo, changed, &provider.diff_budget())
        .and_then(|ctx| privacy.prepare_grouping(ctx))
        .map_err(BuildError::Fatal)?;
    let plan = provider.generate_plan(&ctx).map_err(|e| {
        // A missing or rejected key fails the single-commit fallback identically;
        // do not pretend to recover. Every other provider error degrades to the
        // single-commit path (the simpler message call may still succeed where the
        // json_schema plan call did not). CLO-492 owns richer fallback policy.
        let fatal = matches!(
            e.kind,
            ErrorKind::MissingKey { .. } | ErrorKind::Auth { .. }
        );
        if fatal {
            BuildError::Fatal(GcmError::Provider(e))
        } else {
            let raw_code = output::provider_error_code(&e);
            BuildError::Fallback {
                reason: e.to_string(),
                raw_code,
            }
        }
    })?;
    let change_set: HashSet<String> = changed.iter().map(|c| c.path.clone()).collect();
    plan::validate(&plan, &change_set).map_err(|e| BuildError::Fallback {
        reason: e.to_string(),
        raw_code: output::fallback_raw_code(&e.to_string()),
    })?;
    Ok(plan)
}

/// Display the groups, then (unless `--dry-run` or `--plan-only`) confirm and
/// commit group 1, advancing the cache on a successful commit.
#[allow(clippy::too_many_arguments)]
fn commit_first_group(
    repo: &Repo,
    args: &Cli,
    changed: &[ChangedFile],
    plan: &Plan,
    cached: bool,
    model: &str,
    provider: &dyn Provider,
    provider_name: &str,
    privacy: &Privacy,
) -> Envelope {
    if !args.json && (args.dry_run || args.plan_only) {
        display_groups(plan);
    }
    let group1 = &plan.groups[0];
    let group1_files = select_changed(changed, &group1.files);

    // Resolve group 1's message. A fresh plan (or a full-plan cache hit) already
    // carries it; an advanced cache hit has a null message, so regenerate it
    // per group (ADR-001 #6) via a message-only call scoped to this group's diff,
    // taken BEFORE staging. No grouping call is made here.
    let message = match group1.commit_message.as_deref() {
        Some(m) if !m.trim().is_empty() => m.to_string(),
        _ => {
            let gathered =
                match diff::gather_for_files(repo, &group1_files, &provider.diff_budget())
                    .and_then(|g| privacy.prepare_diff(g))
                {
                    Ok(g) => g,
                    Err(e) => {
                        return output::error(
                            Some(provider_name),
                            Some(model),
                            Some(output::MODE_GROUPED),
                            &e,
                        );
                    }
                };
            match provider.generate_message(&gathered) {
                Ok(m) => m,
                Err(e) => {
                    return output::error(
                        Some(provider_name),
                        Some(model),
                        Some(output::MODE_GROUPED),
                        &GcmError::Provider(e),
                    );
                }
            }
        }
    };

    let changed_paths: Vec<String> = changed.iter().map(|c| c.path.clone()).collect();

    if args.plan_only {
        if !args.json {
            ui::preview_plan(&message, plan.groups.len(), remaining_files(plan));
        }
        return output::plan(
            Some(provider_name),
            Some(model),
            output::MODE_PLAN_ONLY,
            plan.clone(),
            changed_paths,
            cached,
        );
    }

    if args.dry_run {
        if !args.json {
            ui::preview_plan(&message, plan.groups.len(), remaining_files(plan));
        }
        return output::plan(
            Some(provider_name),
            Some(model),
            output::MODE_DRY_RUN,
            plan.clone(),
            changed_paths,
            cached,
        );
    }

    // Capture the pre-run index up front. Restore it only on a *pre-commit-step*
    // failure (FR-47). A commit-step failure (CommitFailed) leaves the group
    // staged for retry (FR-58), so it is NOT restored. Abort never mutates the
    // index, so it needs no restore.
    let snapshot = match repo.snapshot_index() {
        Ok(s) => s,
        Err(e) => {
            return output::error(
                Some(provider_name),
                Some(model),
                Some(output::MODE_GROUPED),
                &e,
            );
        }
    };
    let result = commit_group_flow(repo, args, &group1_files, &message);
    if let Err(e) = &result {
        if !e.leaves_staged() {
            let _ = repo.restore_index(&snapshot);
        }
    }

    // Advance the cache only on a real commit - never on abort or failure.
    if matches!(&result, Ok(CommitOutcome::Committed)) {
        cache::advance(repo, plan, model);
    }

    match result {
        Ok(CommitOutcome::Aborted) => {
            output::noop(Some(provider_name), Some(model), output::MODE_GROUPED)
        }
        Ok(CommitOutcome::Committed) => {
            let hash = match repo.last_commit_hash() {
                Ok(h) => h,
                Err(e) => {
                    return output::error(
                        Some(provider_name),
                        Some(model),
                        Some(output::MODE_GROUPED),
                        &e,
                    );
                }
            };
            output::committed_group(
                Some(provider_name),
                Some(model),
                hash,
                message,
                group1.files.clone(),
                plan.groups.len(),
                remaining_files(plan),
            )
        }
        Err(e) => output::error(
            Some(provider_name),
            Some(model),
            Some(output::MODE_GROUPED),
            &e,
        ),
    }
}

/// Confirm, then clear staging and stage exactly group 1 before committing.
fn commit_group_flow(
    repo: &Repo,
    args: &Cli,
    group1_files: &[&ChangedFile],
    message: &str,
) -> Result<CommitOutcome, GcmError> {
    match ui::confirm(message, args.yes, args.json)? {
        Decision::Abort => Ok(CommitOutcome::Aborted),
        Decision::Commit(final_message) => {
            repo.clear_staged()?;
            repo.stage_group(group1_files)?;
            repo.commit_signed(&final_message)?;
            Ok(CommitOutcome::Committed)
        }
    }
}

/// The single-commit path (CLO-486 tracer): used by `--all`, a clean
/// merge-in-progress, and the grouping fallback. Commits all changes as one.
fn single_commit_path(
    repo: &Repo,
    args: &Cli,
    provider: &dyn Provider,
    changed: &[ChangedFile],
    privacy: &Privacy,
    provider_name: &str,
    model: &str,
) -> Envelope {
    let changed_paths: Vec<String> = changed.iter().map(|c| c.path.clone()).collect();

    if args.plan_only {
        // Non-destructive single-path preview: no provider call needed.
        return output::plan(
            Some(provider_name),
            Some(model),
            output::MODE_SINGLE,
            Plan { groups: vec![] },
            changed_paths,
            false,
        );
    }

    let gathered = match diff::gather_for_changed(repo, changed, &provider.diff_budget())
        .and_then(|g| privacy.prepare_diff(g))
    {
        Ok(g) => g,
        Err(e) => {
            return output::error(
                Some(provider_name),
                Some(model),
                Some(output::MODE_SINGLE),
                &e,
            );
        }
    };
    let message = match provider.generate_message(&gathered) {
        Ok(m) => m,
        Err(e) => {
            return output::error(
                Some(provider_name),
                Some(model),
                Some(output::MODE_SINGLE),
                &GcmError::Provider(e),
            );
        }
    };

    if args.dry_run {
        if !args.json {
            ui_preview(&message);
        }
        return output::plan(
            Some(provider_name),
            Some(model),
            output::MODE_SINGLE,
            Plan { groups: vec![] },
            changed_paths,
            false,
        );
    }

    // `--all`, a clean merge, and the grouping fallback all clear the cached
    // plan (FR-28) - but only on the REAL (non-dry-run, non-plan-only) path.
    cache::clear(repo);
    let snapshot = match repo.snapshot_index() {
        Ok(s) => s,
        Err(e) => {
            return output::error(
                Some(provider_name),
                Some(model),
                Some(output::MODE_SINGLE),
                &e,
            );
        }
    };
    let result = single_commit_flow(repo, args, changed, &message);
    if result.is_err() {
        let _ = repo.restore_index(&snapshot);
    }

    match result {
        Ok(SingleOutcome::Aborted) => {
            output::noop(Some(provider_name), Some(model), output::MODE_SINGLE)
        }
        Ok(SingleOutcome::Committed) => {
            let hash = match repo.last_commit_hash() {
                Ok(h) => h,
                Err(e) => {
                    return output::error(
                        Some(provider_name),
                        Some(model),
                        Some(output::MODE_SINGLE),
                        &e,
                    );
                }
            };
            output::committed(
                Some(provider_name),
                Some(model),
                output::MODE_SINGLE,
                hash,
                message,
                changed_paths,
            )
        }
        Err(e) => output::error(
            Some(provider_name),
            Some(model),
            Some(output::MODE_SINGLE),
            &e,
        ),
    }
}

enum SingleOutcome {
    Committed,
    Aborted,
}

fn single_commit_flow(
    repo: &Repo,
    args: &Cli,
    changed: &[ChangedFile],
    message: &str,
) -> Result<SingleOutcome, GcmError> {
    match ui::confirm(message, args.yes, args.json)? {
        Decision::Abort => Ok(SingleOutcome::Aborted),
        Decision::Commit(final_message) => {
            if repo.is_merging() {
                repo.stage_all()?;
            } else {
                repo.clear_staged()?;
                let files: Vec<&ChangedFile> = changed.iter().collect();
                repo.stage_group(&files)?;
            }
            repo.commit_signed(&final_message)?;
            Ok(SingleOutcome::Committed)
        }
    }
}

/// Run the single-commit fallback after a grouped-plan failure. If the fallback
/// commit succeeds, the envelope is `status: "fallback"`; if it fails, the
/// envelope is `status: "error"`.
#[allow(clippy::too_many_arguments)]
fn run_fallback(
    repo: &Repo,
    args: &Cli,
    provider: &dyn Provider,
    changed: &[ChangedFile],
    privacy: &Privacy,
    provider_name: &str,
    model: &str,
    reason: String,
    raw_code: String,
) -> Envelope {
    if !args.json {
        eprintln!("gcm: {reason}. Falling back to single-commit mode.");
    }
    let env = single_commit_path(repo, args, provider, changed, privacy, provider_name, model);
    if env.status == output::STATUS_COMMITTED {
        // Re-wrap a successful single commit as a fallback envelope, preserving
        // the reason the grouping path was not used.
        if let Some(commit) = env.commit {
            return output::fallback(Some(provider_name), Some(model), reason, raw_code, commit);
        }
    }
    env
}

/// Resolve group 1's file paths back to their `ChangedFile` entries (so rename
/// staging can include the original path). Validation guarantees every path
/// resolves.
fn select_changed<'a>(changed: &'a [ChangedFile], paths: &[String]) -> Vec<&'a ChangedFile> {
    paths
        .iter()
        .filter_map(|p| changed.iter().find(|c| &c.path == p))
        .collect()
}

/// Number of files in groups after the first (committed on later runs).
fn remaining_files(plan: &Plan) -> usize {
    plan.groups.iter().skip(1).map(|g| g.files.len()).sum()
}

fn display_groups(plan: &Plan) {
    println!();
    println!("Found {} group(s):", plan.groups.len());
    for (i, group) in plan.groups.iter().enumerate() {
        println!();
        if i == 0 {
            println!("> Group 1 (committing now): {}", group.summary);
        } else {
            println!("  Group {} (next run): {}", i + 1, group.summary);
        }
        for file in &group.files {
            println!("    {file}");
        }
    }
    println!();
}

fn ui_preview(message: &str) {
    println!();
    println!("Commit message (dry run - nothing staged or committed):");
    println!("-----------------------------");
    println!("{message}");
    println!("-----------------------------");
}

fn noop_mode(args: &Cli) -> &'static str {
    if args.plan_only {
        output::MODE_PLAN_ONLY
    } else if args.dry_run {
        output::MODE_DRY_RUN
    } else {
        output::MODE_PLAN_ONLY
    }
}

fn mode_from_args(args: &Cli) -> &'static str {
    if args.plan_only {
        output::MODE_PLAN_ONLY
    } else if args.dry_run {
        output::MODE_DRY_RUN
    } else if args.all {
        output::MODE_SINGLE
    } else {
        output::MODE_GROUPED
    }
}

fn grouped_mode(args: &Cli) -> Option<&'static str> {
    if args.plan_only {
        Some(output::MODE_PLAN_ONLY)
    } else if args.dry_run {
        Some(output::MODE_DRY_RUN)
    } else {
        Some(output::MODE_GROUPED)
    }
}

/// Human summary printed after a grouped commit. gcm commits the first group of
/// the current run's plan, so this is always "group 1 of N"; when later groups
/// remain it adds how many files/groups are left and the hint to re-run. Mirrors
/// the `--dry-run` preview wording ([`ui::preview_plan`]).
fn grouped_commit_summary(group_count: usize, remaining_files: usize) -> String {
    let mut summary = format!("Committed group 1 of {group_count}.");
    if group_count > 1 {
        summary.push_str(&format!(
            "\n{remaining_files} file(s) remain in {} more group(s); run gcm again to commit the next group.",
            group_count - 1
        ));
    }
    summary
}

fn print_human(env: &Envelope) {
    match env.status {
        output::STATUS_NOOP => {
            if env.mode == Some(output::MODE_GROUPED) || env.mode == Some(output::MODE_SINGLE) {
                println!("Aborted. Nothing staged, nothing committed.");
            } else {
                println!("No changes to commit");
            }
        }
        output::STATUS_PLAN => {
            // The preview itself was already printed during execution (the
            // message is not part of the stable envelope). Nothing to repeat.
        }
        output::STATUS_COMMITTED => {
            if let Some(commit) = &env.commit {
                if env.mode == Some(output::MODE_GROUPED) {
                    match &env.group_progress {
                        Some(gp) => println!(
                            "{}",
                            grouped_commit_summary(gp.group_count, gp.remaining_files)
                        ),
                        None => println!("Committed group 1."),
                    }
                } else {
                    println!("Committed.");
                }
                println!("{} {}", commit.hash, commit.message);
            }
        }
        output::STATUS_FALLBACK => {
            if let Some(commit) = &env.commit {
                println!("Committed.");
                println!("{} {}", commit.hash, commit.message);
            }
        }
        output::STATUS_ERROR => {
            if let Some(err) = &env.error {
                eprintln!("gcm: {}", err.message);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_summary_multi_group_shows_count_and_remaining() {
        assert_eq!(
            grouped_commit_summary(3, 5),
            "Committed group 1 of 3.\n\
             5 file(s) remain in 2 more group(s); run gcm again to commit the next group."
        );
    }

    #[test]
    fn grouped_summary_last_group_omits_remaining_line() {
        assert_eq!(grouped_commit_summary(1, 0), "Committed group 1 of 1.");
    }
}
