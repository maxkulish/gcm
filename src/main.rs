mod cache;
mod cli;
mod debug;
mod diff;
mod error;
mod git;
mod groq;
mod plan;
mod ui;

use std::collections::HashSet;

use clap::Parser;

use cli::Cli;
use error::GcmError;
use git::{ChangedFile, Repo};
use plan::Plan;
use ui::Decision;

fn main() {
    let args = Cli::parse();
    std::process::exit(run(&args));
}

/// Returns the process exit code: 0 = success or user abort, 1 = runtime error
/// (usage errors exit 2 via clap before we get here). See FR-9, FR-39.
fn run(args: &Cli) -> i32 {
    match execute(args) {
        Ok(()) => 0,
        Err(e) => {
            // Surface the typed variant (e.g. Groq(RateLimit { .. })) in debug
            // logs so the error type is visible (CLO-488 acceptance).
            crate::debug_log!("{e:?}");
            eprintln!("gcm: {e}");
            e.exit_code()
        }
    }
}

fn execute(args: &Cli) -> Result<(), GcmError> {
    let repo = Repo::discover()?.ok_or(GcmError::NotARepo)?;

    // `--reset` discards any cached plan up front (FR-8/FR-28), before the
    // no-changes check so it clears even when the tree is currently clean.
    if args.reset {
        cache::clear(&repo);
    }

    if !repo.has_changes()? {
        println!("No changes to commit");
        return Ok(());
    }

    // Fail fast before sending any diff to the provider if we could not confirm
    // the commit anyway (ADR-001 #10, AC-11).
    if ui::needs_terminal_but_absent(args.yes, args.dry_run) {
        return Err(GcmError::NonInteractive);
    }

    // Merge-state guard (CLO-487 review-2 #2) runs BEFORE any grouping bypass,
    // including `--all`: staging a conflicted working tree on *either* path
    // (grouping `add` or single-commit `add -A`) would bake `<<<<<<<` markers
    // into the commit, so an unresolved conflict must abort regardless of flags.
    let changed = repo.changed_files()?;
    if changed.iter().any(|c| c.is_unmerged()) {
        return Err(GcmError::UnmergedConflicts);
    }

    // FR-46: warn before resetting a user-curated index. Both real commit paths
    // discard the user's staging (grouping resets via `clear_staged`; `--all`
    // overrides via `git add -A`), so a pre-existing curated/partial index is
    // about to be lost. Gate on "will the index actually be mutated": today that
    // is `!dry_run`; when CLO-493 adds a no-mutation `--plan-only`, extend this
    // gate (do not duplicate it). Prints even under `--yes` (FR-46: documented,
    // not silent - it is stderr, never a prompt). Placed after the `is_unmerged`
    // guard, so a conflicted entry (whose `x` can read as staged) never reaches
    // it. `--reset` clears only the cache, not the index, so it still warns.
    if !args.dry_run {
        let staged = changed.iter().filter(|c| c.is_staged()).count();
        if staged > 0 {
            let partial = changed.iter().filter(|c| c.is_partially_staged()).count();
            eprintln!("{}", ui::curated_index_warning(staged, partial));
        }
    }

    // `--all`, or a clean merge-in-progress, bypasses grouping and commits
    // everything as one. A clean `MERGE_HEAD` makes `git commit` finalize the
    // merge as a proper two-parent merge commit. The single-commit path clears
    // the cached plan (FR-28).
    if args.all || repo.is_merging() {
        return single_commit(&repo, args);
    }

    // Grouping path. A fresh plan is persisted to the per-repo cache; a cache
    // hit reuses it and skips the grouping call entirely (FR-25/FR-2). The
    // model is folded into the freshness fingerprint (FR-27). A structured-
    // output/parse/validation failure falls back to the single-commit path with
    // an announced reason (never silent); a fatal error (missing key, git
    // failure) is returned as-is.
    let model = groq::resolved_model();
    let plan = match cache::load(&repo, &changed, &model) {
        Some(plan) => {
            // Defense in depth (CLO-492, FR-23): a cached plan must still
            // partition the CURRENT change set before it drives grouped commits.
            // A plan written by a pre-CLO-492 binary (which only screened unknown
            // files) - or any future advance defect - could otherwise replay an
            // omission/duplicate and silently drop a file. `validate_cached` skips
            // the groups[0] message check (an advanced entry has a null first
            // message by design, regenerated per group). On failure, drop the
            // stale entry and take the same announced fallback as a fresh-plan
            // validation failure.
            let change_set: HashSet<String> = changed.iter().map(|c| c.path.clone()).collect();
            if let Err(reason) = plan::validate_cached(&plan, &change_set) {
                eprintln!(
                    "gcm: cached plan invalid ({reason}). Falling back to single-commit mode."
                );
                cache::clear(&repo);
                return single_commit(&repo, args);
            }
            plan
        }
        None => match build_plan(&repo, &changed) {
            Ok(plan) => {
                // Save the full plan even on a `--dry-run` (FR-7: dry-run
                // uses/saves but does not advance); advancement is gated later.
                cache::save(&repo, &plan, &changed, &model);
                plan
            }
            Err(BuildError::Fatal(e)) => return Err(e),
            Err(BuildError::Fallback(reason)) => {
                eprintln!("gcm: {reason}. Falling back to single-commit mode.");
                return single_commit(&repo, args);
            }
        },
    };

    commit_first_group(&repo, args, &changed, &plan, &model)
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
    Fallback(String),
}

/// Gather the grouping context, request the plan, and basic-validate it.
/// Model/plan failures (structured-output error, unparseable JSON, empty
/// response, validation) are `Fallback`; a missing key or git failure is
/// `Fatal`.
fn build_plan(repo: &Repo, changed: &[ChangedFile]) -> Result<Plan, BuildError> {
    let ctx = diff::gather_for_grouping(repo, changed).map_err(BuildError::Fatal)?;
    let plan = groq::generate_plan(&ctx).map_err(|e| match e {
        // A missing or rejected key fails the single-commit fallback identically;
        // do not pretend to recover. Every other provider error degrades to the
        // single-commit path (the simpler message call may still succeed where the
        // json_schema plan call did not). CLO-492 owns richer fallback policy.
        groq::GroqError::MissingKey | groq::GroqError::Auth(_) => {
            BuildError::Fatal(GcmError::Groq(e))
        }
        other => BuildError::Fallback(other.to_string()),
    })?;
    let change_set: HashSet<String> = changed.iter().map(|c| c.path.clone()).collect();
    plan::validate(&plan, &change_set).map_err(|e| BuildError::Fallback(e.to_string()))?;
    Ok(plan)
}

/// Display the groups, then (unless `--dry-run`) confirm and commit group 1,
/// advancing the cache on a successful commit.
fn commit_first_group(
    repo: &Repo,
    args: &Cli,
    changed: &[ChangedFile],
    plan: &Plan,
    model: &str,
) -> Result<(), GcmError> {
    display_groups(plan);
    let group1 = &plan.groups[0];
    let group1_files = select_changed(changed, &group1.files);

    // Resolve group 1's message. A fresh plan (or a full-plan cache hit) already
    // carries it; an advanced cache hit has a null message, so regenerate it
    // per group (ADR-001 #6) via a message-only call scoped to this group's diff,
    // taken BEFORE staging. No grouping call is made here.
    let message = match group1.commit_message.as_deref() {
        Some(m) if !m.trim().is_empty() => m.to_string(),
        _ => groq::generate_commit_message(&diff::gather_for_files(repo, &group1_files)?)?,
    };

    if args.dry_run {
        ui::preview_plan(&message, plan.groups.len(), remaining_files(plan));
        return Ok(());
    }

    // Capture the pre-run index up front. Restore it only on a *pre-commit-step*
    // failure (FR-47). A commit-step failure (CommitFailed) leaves the group
    // staged for retry (FR-58), so it is NOT restored. Abort never mutates the
    // index, so it needs no restore.
    let snapshot = repo.snapshot_index()?;
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
    result.map(|_| ())
}

/// Confirm, then clear staging and stage exactly group 1 before committing.
fn commit_group_flow(
    repo: &Repo,
    args: &Cli,
    group1_files: &[&ChangedFile],
    message: &str,
) -> Result<CommitOutcome, GcmError> {
    match ui::confirm(message, args.yes)? {
        Decision::Abort => {
            println!("Aborted. Nothing staged, nothing committed.");
            Ok(CommitOutcome::Aborted)
        }
        Decision::Commit(final_message) => {
            repo.clear_staged()?;
            repo.stage_group(group1_files)?;
            repo.commit_signed(&final_message)?;
            println!("Committed group 1.");
            Ok(CommitOutcome::Committed)
        }
    }
}

/// The single-commit path (CLO-486 tracer): used by `--all`, a clean
/// merge-in-progress, and the grouping fallback. Commits all changes as one.
fn single_commit(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
    if args.dry_run {
        let gathered = diff::gather(repo)?;
        let message = groq::generate_commit_message(&gathered)?;
        ui_preview(&message);
        return Ok(());
    }
    // `--all`, a clean merge, and the grouping fallback all clear the cached
    // plan (FR-28) - but only on the REAL (non-dry-run) path. A `--dry-run`
    // (incl. `--all --dry-run` and a dry-run fallback) returns above and clears
    // nothing: a preview must mutate no state (FR-7). A stale cache left behind
    // by a dry-run is harmless - the next real run re-validates the fingerprint
    // and re-analyzes on a mismatch.
    cache::clear(repo);
    let snapshot = repo.snapshot_index()?;
    let result = single_commit_flow(repo, args);
    if result.is_err() {
        let _ = repo.restore_index(&snapshot);
    }
    result
}

fn single_commit_flow(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
    let gathered = diff::gather(repo)?;
    let message = groq::generate_commit_message(&gathered)?;
    match ui::confirm(&message, args.yes)? {
        Decision::Abort => {
            println!("Aborted. Nothing staged, nothing committed.");
            Ok(())
        }
        Decision::Commit(final_message) => {
            repo.stage_all()?;
            repo.commit_signed(&final_message)?;
            println!("Committed.");
            Ok(())
        }
    }
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
