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

    // `--all`, or a clean merge-in-progress, bypasses grouping and commits
    // everything as one. A clean `MERGE_HEAD` makes `git commit` finalize the
    // merge as a proper two-parent merge commit.
    if args.all || repo.is_merging() {
        return single_commit(&repo, args);
    }

    // Grouping path. A structured-output/parse/validation failure falls back to
    // the single-commit path with an announced reason (never silent). A fatal
    // error (missing key, git failure) is returned as-is - the single-commit
    // path would hit the same wall, so there is nothing to fall back to.
    let plan = match build_plan(&repo, &changed) {
        Ok(plan) => plan,
        Err(BuildError::Fatal(e)) => return Err(e),
        Err(BuildError::Fallback(reason)) => {
            eprintln!("gcm: {reason}. Falling back to single-commit mode.");
            return single_commit(&repo, args);
        }
    };

    commit_first_group(&repo, args, &changed, &plan)
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
    plan::validate_basic(&plan, &change_set).map_err(|e| BuildError::Fallback(e.to_string()))?;
    Ok(plan)
}

/// Display the groups, then (unless `--dry-run`) confirm and commit group 1.
fn commit_first_group(
    repo: &Repo,
    args: &Cli,
    changed: &[ChangedFile],
    plan: &Plan,
) -> Result<(), GcmError> {
    display_groups(plan);
    let group1 = &plan.groups[0]; // validated non-empty with a usable message
    let message = group1.commit_message.clone().unwrap_or_default();

    if args.dry_run {
        ui::preview_plan(&message, plan.groups.len(), remaining_files(plan));
        return Ok(());
    }

    let group1_files = select_changed(changed, &group1.files);

    // Capture the pre-run index up front; restore on any post-snapshot failure
    // (FR-47). Abort never mutates the index, so it needs no restore.
    let snapshot = repo.snapshot_index()?;
    let result = commit_group_flow(repo, args, &group1_files, &message);
    if result.is_err() {
        let _ = repo.restore_index(&snapshot);
    }
    result
}

/// Confirm, then clear staging and stage exactly group 1 before committing.
fn commit_group_flow(
    repo: &Repo,
    args: &Cli,
    group1_files: &[&ChangedFile],
    message: &str,
) -> Result<(), GcmError> {
    match ui::confirm(message, args.yes)? {
        Decision::Abort => {
            println!("Aborted. Nothing staged, nothing committed.");
            Ok(())
        }
        Decision::Commit(final_message) => {
            repo.clear_staged()?;
            repo.stage_group(group1_files)?;
            repo.commit_signed(&final_message)?;
            println!("Committed group 1.");
            Ok(())
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
