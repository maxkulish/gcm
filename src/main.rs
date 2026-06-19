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

    let gathered = diff::gather(&repo)?;
    let message = groq::generate_commit_message(&gathered)?;

    if args.dry_run {
        ui_preview(&message);
        return Ok(());
    }

    match ui::confirm(&message, args.yes)? {
        Decision::Abort => {
            println!("Aborted. Nothing staged, nothing committed.");
            Ok(())
        }
        Decision::Commit(final_message) => commit_transactionally(&repo, &final_message),
    }
}

/// Stage and commit within an index transaction: snapshot the index first, and
/// restore it if staging or the signed commit fails (FR-47, AC-13).
fn commit_transactionally(repo: &Repo, message: &str) -> Result<(), GcmError> {
    let snapshot = repo.snapshot_index()?;

    let result = repo.stage_all().and_then(|()| repo.commit_signed(message));

    if result.is_err() {
        // Best-effort restore; surface the original error regardless.
        let _ = repo.restore_index(&snapshot);
    }
    result?;

    println!("Committed.");
    Ok(())
}

fn ui_preview(message: &str) {
    println!();
    println!("Commit message (dry run - nothing staged or committed):");
    println!("-----------------------------");
    println!("{message}");
    println!("-----------------------------");
}
