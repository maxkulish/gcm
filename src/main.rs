mod cli;
mod diff;
mod error;
mod git;
mod groq;
mod ui;

use clap::Parser;

use cli::Cli;
use error::GcmError;
use git::Repo;
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

    // Dry run never stages, so it needs no index transaction.
    if args.dry_run {
        let gathered = diff::gather(&repo)?;
        let message = groq::generate_commit_message(&gathered)?;
        ui_preview(&message);
        return Ok(());
    }

    // Commit path: capture the pre-run index as a tree up front, before gathering
    // the diff or prompting, so the restore point is the true pre-run state even
    // if the index changes while the user is at the prompt. Restore it on any
    // post-snapshot failure (FR-47, AC-13). User abort never mutates the index
    // (staging happens only just before commit), so it needs no restore.
    let snapshot = repo.snapshot_index()?;
    let result = commit_flow(&repo, args);
    if result.is_err() {
        let _ = repo.restore_index(&snapshot);
    }
    result
}

/// Gather, generate, confirm, then stage and commit. Any `Err` returned here is
/// the trigger for the caller to restore the index.
fn commit_flow(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
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

fn ui_preview(message: &str) {
    println!();
    println!("Commit message (dry run - nothing staged or committed):");
    println!("-----------------------------");
    println!("{message}");
    println!("-----------------------------");
}
