//! `gcm resolve` — LLM-assisted merge conflict resolver (Phase 1: local markers).
//!
//! Public entry point is [`run_resolve`] (called from `main.rs` for the
//! `resolve` subcommand). All sub-modules are implementation details.

pub mod classify;
pub mod markers;
pub mod mergiraf;
pub mod prompt;
pub mod remote;
pub use remote::run_resolve_remote_opt;
pub mod report;
pub mod validate;

use std::collections::{HashMap, HashSet};

use crate::cli::{Cli, Commands};
use crate::config::{AutoPolicy, ConflictConfig};
use crate::error::GcmError;
use crate::git::{ChangedFile, Repo};
use crate::output;
use crate::privacy::Privacy;
use crate::provider::{ConflictHunk, Provider, Resolution, ResolveContext};
use gcm::privacy::SecretScanMode;

use crate::git::FinishOutcome;
use classify::{classify, HunkResolution};
use markers::{has_conflict_markers, parse, ConflictFile};
use report::{FileAction, FileReport, FinishReport, FinishResult, ResolveReport, ResolveStatus};
use validate::validate;

/// Byte-exact snapshot of the unmerged working-tree files, captured before the
/// first mutation (the zdiff3 re-checkout). A user rejection restores these
/// bytes so the repository leaves the run exactly as it entered it - including
/// any manual partial resolution the user had made before running gcm.
struct WorkingTreeSnapshot {
    /// path -> pre-run bytes, in capture order.
    original: Vec<(String, Vec<u8>)>,
    /// path -> the bytes gcm last wrote (zdiff3/mergiraf output), recorded
    /// after the propose phase. The restore guard compares against these so a
    /// concurrent external edit is never overwritten.
    written: HashMap<String, Vec<u8>>,
}

impl WorkingTreeSnapshot {
    fn capture(repo: &Repo, paths: &[String]) -> Result<Self, GcmError> {
        let mut original = Vec::with_capacity(paths.len());
        for p in paths {
            original.push((p.clone(), repo.read_file_bytes(p)?));
        }
        Ok(Self {
            original,
            written: HashMap::new(),
        })
    }

    /// Record the current on-disk bytes as gcm's own writes. Called once the
    /// propose phase has finished mutating files.
    fn record_written(&mut self, repo: &Repo) -> Result<(), GcmError> {
        for (p, _) in &self.original {
            self.written.insert(p.clone(), repo.read_file_bytes(p)?);
        }
        Ok(())
    }

    /// Restore every snapshotted file's pre-run bytes. A file whose current
    /// content is not what gcm last wrote (edited or deleted in another
    /// terminal mid-run) is skipped with a warning rather than clobbered.
    fn restore(&self, repo: &Repo) -> Result<(), GcmError> {
        for (p, bytes) in &self.original {
            if let Some(expected) = self.written.get(p) {
                let untouched =
                    matches!(repo.read_file_bytes(p), Ok(current) if &current == expected);
                if !untouched {
                    eprintln!(
                        "gcm resolve: warning: {p} was modified outside gcm during the run - leaving it as-is"
                    );
                    continue;
                }
            }
            repo.write_file_bytes(p, bytes)?;
        }
        Ok(())
    }
}

/// What the propose phase produced for one unmerged file. Beyond the zdiff3
/// and mergiraf mutations (covered by the snapshot), building these performs
/// no working-tree write, no staging, and no prompting - the confirm and
/// apply phases own those.
struct FileProposal {
    path: String,
    hunks_total: usize,
    hunks_auto: usize,
    hunks_llm: usize,
    hunks_escalated: usize,
    kind: ProposalKind,
}

enum ProposalKind {
    /// Resolved text awaiting the user's confirmation.
    Resolved { text: String },
    /// Already marker-free on disk (resolved manually before the run): staged
    /// as-is in the apply phase without a prompt - the content is the user's
    /// own work, there is nothing to accept or restore.
    AlreadyResolved,
    /// Hunk-level tool escalation (provider gaps or a failed validation
    /// retry): left conflicted with its markers in place. Reported as
    /// `dry_run` in dry-run mode, matching the pre-transaction report.
    EscalatedHunks,
    /// Whole-file escalation decided before any hunk work (binary file or
    /// sensitive path). Reported as `escalated` even in dry-run mode.
    EscalatedFile,
    /// Excluded by `.gcmignore`/`gcmignore`.
    Skipped,
}

/// Execution context for the resolution engine. The engine stages and
/// finishes only in `Local` mode; in `Remote` mode the wrapper stays the sole
/// committer of the scratch repo, and a clean merge (no unmerged files) is a
/// successful noop rather than a user error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveMode {
    Local,
    Remote,
}

/// Entry point for `gcm resolve`.
pub fn run_resolve(args: &Cli) -> Result<(), GcmError> {
    let repo = Repo::discover()?.ok_or(GcmError::NotARepo)?;
    let report = run_resolve_loop(&repo, args)?;
    if args.json {
        report::emit(&report);
    } else {
        print_human_report(&report);
    }
    Ok(())
}

/// The sequencing head of a stopped rebase/cherry-pick: (operation, short sha).
type OpHead = (&'static str, String);

/// Drive a rebase or cherry-pick sequence to completion, one transaction per
/// conflict stop (CLO-554).
///
/// The engine itself stays single-round: this is the only place that reads
/// `StoppedOnConflict` as "keep going" rather than "we are done". Between
/// rounds it enforces the two boundaries that keep provider spend honest - the
/// round cap, and (interactively) a gate the user answers *before* the next
/// round's propose phase issues any LLM call.
///
/// A merge never sequences, so it runs exactly one round and leaves the
/// envelope byte-identical to the pre-loop output.
fn run_resolve_loop(repo: &Repo, args: &Cli) -> Result<ResolveReport, GcmError> {
    // Read the cap up front. This also validates a config-supplied 0 before the
    // first round touches the working tree (clap already rejected a flag 0).
    let max_rounds = resolve_conflict_config(args)?.max_rounds;

    let mut rounds: Vec<report::RoundReport> = Vec::new();
    let mut head: Option<OpHead> = repo.conflict_op_head();
    // Where the operation is parked when the loop exits mid-sequence.
    let mut stopped_on: Option<String> = None;

    let (mut envelope, terminal) = loop {
        let round_no = rounds.len() + 1;
        print_round_banner(repo, round_no, max_rounds, head.as_ref())?;

        let round = run_resolve_in_repo(repo, args, ResolveMode::Local)?;
        print_round_summary(repo, round_no, &round);

        rounds.push(report::RoundReport {
            round: round_no,
            commit: head.as_ref().map(|(_, sha)| sha.clone()),
            status: round.status,
            files: round.files.clone(),
            staged: round.staged.clone(),
            finish: round.finish.clone(),
        });

        let stopped_again = matches!(
            round.finish.as_ref().map(|f| f.result),
            Some(FinishResult::StoppedOnConflict)
        );

        if !stopped_again {
            // The round itself ended the run: nothing left to continue onto.
            let terminal = match round.status {
                ResolveStatus::Aborted => report::LoopTerminal::Aborted,
                ResolveStatus::Partial => report::LoopTerminal::Partial,
                _ => report::LoopTerminal::Completed,
            };
            break (round, terminal);
        }

        if rounds.len() as u32 >= max_rounds {
            stopped_on = repo.conflict_op_head().map(|(_, sha)| sha);
            break (round, report::LoopTerminal::CapReached);
        }

        // Describe the next stop with the best information available now, then
        // ask. Under --yes the gate is skipped entirely; a non-TTY run without
        // --yes never reaches here, having already failed round 1's terminal
        // check (ADR-001 #10).
        if !args.yes && !gate_next_round(repo, round_no + 1, max_rounds)? {
            stopped_on = repo.conflict_op_head().map(|(_, sha)| sha);
            break (round, report::LoopTerminal::Declined);
        }

        // Authoritative read: taken AFTER the gate is answered, so a repo the
        // user changed from another terminal while the prompt sat open is seen
        // rather than papered over by the pre-prompt value.
        let next_head = repo.conflict_op_head();
        if !advanced(head.as_ref(), next_head.as_ref()) {
            eprintln!(
                "gcm resolve: the {} did not advance past {} - stopping instead of resolving the same commit again.",
                head.as_ref().map(|(op, _)| *op).unwrap_or("operation"),
                head.as_ref().map(|(_, sha)| sha.as_str()).unwrap_or("HEAD")
            );
            // Prefer the fresh read, but fall back to the round's own head so a
            // vanished sequencing ref still names where the run got stuck.
            stopped_on = next_head
                .map(|(_, sha)| sha)
                .or_else(|| head.as_ref().map(|(_, sha)| sha.clone()));
            break (round, report::LoopTerminal::NoProgress);
        }
        head = next_head;
    };

    // The loop block appears only when the driver actually did something a
    // single-round run would not have: ran more than once, or stopped at its
    // own boundary. Everything else emits the pre-CLO-554 envelope unchanged.
    let engaged = rounds.len() > 1
        || matches!(
            terminal,
            report::LoopTerminal::CapReached | report::LoopTerminal::Declined
        );
    if engaged {
        envelope.loop_report = Some(report::LoopReport {
            rounds_run: rounds.len(),
            max_rounds,
            terminal,
            stopped_on,
            rounds,
        });
    }
    Ok(envelope)
}

/// Whether the sequencing operation moved on between rounds.
///
/// Pure so the stall case is testable without provoking a wedged rebase. A head
/// that vanished, or that still names the same commit, means the round changed
/// nothing - spending another round on it would loop forever.
fn advanced(prev: Option<&OpHead>, now: Option<&OpHead>) -> bool {
    match (prev, now) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some((_, p)), Some((_, n))) => p != n,
    }
}

/// Round banner, printed before the propose phase spends anything. Suppressed
/// when there is nothing conflicted, so a no-op or error run reads as it did
/// before the loop existed.
fn print_round_banner(
    repo: &Repo,
    round: usize,
    cap: u32,
    head: Option<&OpHead>,
) -> Result<(), GcmError> {
    let files = repo.unmerged_files()?.len();
    if files == 0 {
        return Ok(());
    }
    let noun = if files == 1 { "file" } else { "files" };
    match head {
        Some((op, sha)) => eprintln!(
            "gcm resolve: round {round} (cap {cap}) - {op} applying {sha}, {files} conflicted {noun}"
        ),
        None => eprintln!("gcm resolve: round {round} (cap {cap}) - {files} conflicted {noun}"),
    }
    Ok(())
}

/// Per-round spend and outcome, so a long loop stays legible as it runs.
fn print_round_summary(repo: &Repo, round: usize, r: &ResolveReport) {
    let (total, auto, llm, escalated) = r.files.iter().fold((0, 0, 0, 0), |a, f| {
        (
            a.0 + f.hunks_total,
            a.1 + f.hunks_auto,
            a.2 + f.hunks_llm,
            a.3 + f.hunks_escalated,
        )
    });
    let tail = match r.finish.as_ref() {
        Some(f) if f.result == FinishResult::Completed => {
            let op = f.op.as_deref().unwrap_or("operation");
            let sha = f.commit.as_deref().unwrap_or("HEAD");
            format!(" - {op} completed ({sha})")
        }
        Some(f) if f.result == FinishResult::StoppedOnConflict => {
            // The finish report carries no sha for this outcome; HEAD is the
            // step this round just committed.
            let op = f.op.as_deref().unwrap_or("operation");
            match repo.head_short_sha() {
                Ok(sha) => format!(" - {op} step committed ({sha})"),
                Err(_) => format!(" - {op} step committed"),
            }
        }
        _ => String::new(),
    };
    eprintln!(
        "gcm resolve: round {round} done - {total} hunks ({auto} auto, {llm} LLM, {escalated} escalated){tail}"
    );
}

/// The round gate: name the stop about to be worked on, then ask. Answering No
/// (or Enter, or EOF) leaves the operation exactly where it is, having spent
/// nothing on it.
fn gate_next_round(repo: &Repo, round: usize, cap: u32) -> Result<bool, GcmError> {
    let files = repo.unmerged_files()?.len();
    let noun = if files == 1 { "file" } else { "files" };
    let summary = match repo.conflict_op_head() {
        Some((op, sha)) => {
            format!("gcm resolve: {op} stopped again on {sha}, {files} conflicted {noun}.")
        }
        None => format!("gcm resolve: stopped again, {files} conflicted {noun}."),
    };
    crate::ui::confirm_round(
        &summary,
        &format!("Resolve round {round} (cap {cap}) with the provider? [y/N] "),
    )
}

/// Core resolution engine used by both the local and remote paths.
///
/// Local callers discover the repo first; remote callers build a scratch repo
/// and pass it in. Returns a `ResolveReport` rather than printing it, so the
/// caller decides how to present the result and can attach remote metadata.
pub fn run_resolve_in_repo(
    repo: &Repo,
    args: &Cli,
    mode: ResolveMode,
) -> Result<ResolveReport, GcmError> {
    let has_state = repo.has_conflict_state();
    let unmerged = repo.unmerged_files()?;

    if mode == ResolveMode::Remote {
        // Remote path: a clean merge (no unmerged files) is a success regardless
        // of whether MERGE_HEAD is set — `git merge --no-ff --no-commit` sets
        // MERGE_HEAD even when the merge produces no conflicts.
        if unmerged.is_empty() {
            return Ok(ResolveReport {
                v: output::SCHEMA_VERSION,
                status: ResolveStatus::Noop,
                files: vec![],
                staged: vec![],
                finish: None,
                restored: false,
                remote: None,
                loop_report: None,
            });
        }
    } else {
        // Local path: no conflict state at all is a user error.
        if !has_state {
            return Err(GcmError::NoConflictInProgress);
        }
        // Has merge state but no unmerged files (e.g. clean merge with
        // --no-commit) — also an error for the local path.
        if unmerged.is_empty() {
            return Err(GcmError::NoConflicts);
        }
    }

    // Hydrate config so provider/model/env precedence works as usual.
    if let Some(cfg) = crate::config::load() {
        crate::config::apply_to_env(&cfg);
    }

    let conflict = resolve_conflict_config(args)?;

    let binary = repo.binary_unmerged_files()?;
    let binary_set: HashSet<String> = binary.into_iter().collect();

    // Files that are already marker-free BEFORE any mutation were resolved by
    // hand before this run. They are excluded from the zdiff3 re-checkout,
    // which would regenerate their markers from the index and destroy the
    // manual resolution; the apply phase stages them as-is. Binary files
    // never qualify - their working copy is not a hand-resolution.
    let mut marker_free: HashSet<String> = HashSet::new();
    for path in &unmerged {
        if binary_set.contains(path) {
            continue;
        }
        let bytes = repo.read_file_bytes(path)?;
        let content = String::from_utf8_lossy(&bytes);
        if parse(path.clone(), &content).hunks.is_empty() {
            marker_free.insert(path.clone());
        }
    }

    // Every failure-prone precondition runs BEFORE the first working-tree
    // mutation, so an early exit (missing key, non-TTY, bad privacy config)
    // leaves the user's files exactly as it found them.
    let provider = crate::provider::select(args.provider, args.model.as_deref())
        .map_err(GcmError::Provider)?;
    let privacy = Privacy::load(repo, args.secret_scan)?;

    // Non-interactive guard: if we would need to prompt but can't, error early.
    if crate::ui::needs_terminal_but_absent(args.yes, args.dry_run) {
        return Err(GcmError::NonInteractive);
    }

    // Snapshot the pre-run bytes of every unmerged file, then re-checkout the
    // still-conflicted ones with zdiff3 markers so every file has a parseable
    // base/ours/theirs. The snapshot makes the zdiff3/mergiraf mutations
    // reversible when the user rejects the transaction. Dry-run never
    // mutates, so it snapshots nothing.
    let mut snapshot = None;
    if !args.dry_run {
        snapshot = Some(WorkingTreeSnapshot::capture(repo, &unmerged)?);
        let paths: Vec<&str> = unmerged
            .iter()
            .filter(|p| !marker_free.contains(*p))
            .map(String::as_str)
            .collect();
        if !paths.is_empty() {
            repo.checkout_conflict_zdiff3(&paths)?;
        }
    }

    // Phase A - propose: build one proposal per file. All provider calls,
    // validation retries, and mergiraf runs happen here; nothing is confirmed,
    // written back, or staged yet.
    let mut proposals = Vec::new();
    for path in &unmerged {
        let changed = ChangedFile {
            x: b'U',
            y: b'U',
            path: path.clone(),
            orig_path: None,
        };
        if privacy.filter_changed(&[changed]).is_empty() {
            eprintln!("gcm resolve: skipping {path} (excluded by .gcmignore/gcmignore)");
            proposals.push(FileProposal {
                path: path.clone(),
                hunks_total: 0,
                hunks_auto: 0,
                hunks_llm: 0,
                hunks_escalated: 0,
                kind: ProposalKind::Skipped,
            });
            continue;
        }

        if marker_free.contains(path) {
            proposals.push(FileProposal {
                path: path.clone(),
                hunks_total: 0,
                hunks_auto: 0,
                hunks_llm: 0,
                hunks_escalated: 0,
                kind: ProposalKind::AlreadyResolved,
            });
            continue;
        }

        proposals.push(propose_file(
            repo,
            path,
            &conflict,
            &binary_set,
            provider.as_ref(),
            &privacy,
            args,
        )?);
    }
    // From here on, on-disk differences from these recorded bytes mean an
    // external edit, which the restore guard must not clobber.
    if let Some(s) = snapshot.as_mut() {
        s.record_written(repo)?;
    }

    if args.dry_run {
        let files: Vec<FileReport> = proposals
            .iter()
            .map(|p| {
                let action = match &p.kind {
                    ProposalKind::Resolved { .. } | ProposalKind::EscalatedHunks => {
                        FileAction::DryRun
                    }
                    ProposalKind::AlreadyResolved => FileAction::Accepted,
                    ProposalKind::EscalatedFile => FileAction::Escalated,
                    ProposalKind::Skipped => FileAction::Skipped,
                };
                file_report(p, action)
            })
            .collect();
        return Ok(report_for(files));
    }

    // Phase B - confirm: collect a decision for every proposal before anything
    // is applied. Any rejection aborts the whole run and restores the pre-run
    // working tree - ownership goes back to the user, exit 0.
    let mut decisions: Vec<Option<FileAction>> = vec![None; proposals.len()];
    let mut texts: Vec<Option<String>> = proposals
        .iter()
        .map(|p| match &p.kind {
            ProposalKind::Resolved { text } => Some(text.clone()),
            _ => None,
        })
        .collect();

    for i in 0..proposals.len() {
        let Some(text) = texts[i].clone() else {
            continue;
        };
        let path = proposals[i].path.clone();
        if args.yes {
            decisions[i] = Some(FileAction::Accepted);
            continue;
        }
        match crate::ui::confirm_file(&path, &text, args.json)? {
            crate::ui::FileDecision::Accept => decisions[i] = Some(FileAction::Accepted),
            crate::ui::FileDecision::Edit => {
                let edited = crate::ui::edit_in_editor(&text)?;
                match validate(&edited, conflict.validate_cmd.as_deref(), repo, &path) {
                    Ok(()) => {
                        texts[i] = Some(edited);
                        decisions[i] = Some(FileAction::Edited);
                    }
                    Err(e) => {
                        // Escalate, never abort (AC5): earlier confirmations
                        // stay valid, this file keeps its markers, and the
                        // run reports Partial.
                        eprintln!(
                            "gcm resolve: {path}: edited content failed validation ({e:?}); escalating this file"
                        );
                        texts[i] = None;
                        decisions[i] = Some(FileAction::Escalated);
                    }
                }
            }
            crate::ui::FileDecision::Skip => {
                decisions[i] = Some(FileAction::Rejected);
                if let Some(s) = snapshot.as_ref() {
                    s.restore(repo)?;
                }
                let files: Vec<FileReport> = proposals
                    .iter()
                    .enumerate()
                    .map(|(j, p)| {
                        let action = match &p.kind {
                            // Decisions made up to the abort; undecided
                            // proposals were never acted on.
                            ProposalKind::Resolved { .. } => {
                                decisions[j].unwrap_or(FileAction::Skipped)
                            }
                            ProposalKind::AlreadyResolved => FileAction::Skipped,
                            ProposalKind::EscalatedHunks | ProposalKind::EscalatedFile => {
                                FileAction::Escalated
                            }
                            ProposalKind::Skipped => FileAction::Skipped,
                        };
                        file_report(p, action)
                    })
                    .collect();
                let mut report = report_for(files);
                report.status = ResolveStatus::Aborted;
                report.restored = true;
                return Ok(report);
            }
        }
    }

    // Phase C - apply: write every confirmed resolution, then stage all
    // resolved paths in one pass keyed by final action - LLM-resolved,
    // edited, mergiraf-resolved, and already-marker-free files alike. In
    // Remote mode the engine stays write-only: the wrapper owns staging and
    // the commit in its scratch repo (AC8).
    let mut files = Vec::with_capacity(proposals.len());
    let mut staged: Vec<String> = Vec::new();
    for (i, p) in proposals.iter().enumerate() {
        let action = match &p.kind {
            ProposalKind::Resolved { .. } => {
                let action = decisions[i].expect("every resolved proposal was decided");
                if action == FileAction::Escalated {
                    // Edited content failed validation: markers kept.
                    action
                } else {
                    let text = texts[i].as_ref().expect("resolved text present");
                    repo.write_file(&p.path, text)?;
                    staged.push(p.path.clone());
                    action
                }
            }
            ProposalKind::AlreadyResolved => {
                staged.push(p.path.clone());
                FileAction::Accepted
            }
            ProposalKind::EscalatedHunks | ProposalKind::EscalatedFile => FileAction::Escalated,
            ProposalKind::Skipped => FileAction::Skipped,
        };
        files.push(file_report(p, action));
    }
    if mode == ResolveMode::Local && !staged.is_empty() {
        let refs: Vec<&str> = staged.iter().map(String::as_str).collect();
        repo.stage_paths(&refs)?;
    }

    let mut report = report_for(files);
    if mode == ResolveMode::Local {
        report.staged = staged;
    }

    // Finish (local only): with every file confirmed and nothing escalated,
    // complete the operation with a signed commit/continue. Escalations and
    // --no-finish stop after staging; the remote wrapper owns its own commit.
    if mode == ResolveMode::Local {
        let op_name = if repo.is_rebasing() {
            Some("rebase")
        } else if repo.is_cherry_picking() {
            Some("cherry-pick")
        } else if repo.is_merging() {
            Some("merge")
        } else {
            None
        };
        if report.status != ResolveStatus::Resolved || args.no_finish() {
            report.finish = Some(FinishReport {
                result: FinishResult::Skipped,
                commit: None,
                op: op_name.map(str::to_string),
            });
        } else {
            report.finish = Some(match repo.finish_conflict_op()? {
                FinishOutcome::Completed { head_sha } => FinishReport {
                    result: FinishResult::Completed,
                    commit: Some(head_sha),
                    op: op_name.map(str::to_string),
                },
                FinishOutcome::StoppedOnNextConflict => FinishReport {
                    result: FinishResult::StoppedOnConflict,
                    commit: None,
                    op: op_name.map(str::to_string),
                },
                FinishOutcome::NothingToFinish => FinishReport {
                    result: FinishResult::Skipped,
                    commit: None,
                    op: None,
                },
                FinishOutcome::Failed { op } => {
                    return Err(GcmError::FinishFailed {
                        op: op.to_string(),
                        detail: "the finishing command failed (see output above)".to_string(),
                    });
                }
            });
        }
    }

    Ok(report)
}

/// Build the per-file report row from a proposal and its final action.
fn file_report(p: &FileProposal, action: FileAction) -> FileReport {
    FileReport {
        path: p.path.clone(),
        hunks_total: p.hunks_total,
        hunks_auto: p.hunks_auto,
        hunks_llm: p.hunks_llm,
        hunks_escalated: p.hunks_escalated,
        action,
    }
}

/// Assemble a report with the status derived from the per-file actions
/// (`Noop` / `Partial` / `Resolved`); callers override status for abort.
fn report_for(files: Vec<FileReport>) -> ResolveReport {
    let any_incomplete = files.iter().any(|f| {
        matches!(
            f.action,
            FileAction::Skipped | FileAction::Escalated | FileAction::DryRun
        )
    });
    let status = if files.is_empty() {
        ResolveStatus::Noop
    } else if any_incomplete {
        ResolveStatus::Partial
    } else {
        ResolveStatus::Resolved
    };
    ResolveReport {
        v: output::SCHEMA_VERSION,
        status,
        files,
        staged: vec![],
        finish: None,
        restored: false,
        remote: None,
        loop_report: None,
    }
}

/// Merge CLI overrides over the config file for the `[conflict]` table.
///
/// Returns `Result` because the round cap is validated here: clap rejects
/// `--max-rounds 0` at parse time, but a `max_rounds = 0` in config.toml only
/// becomes visible once the file is loaded, and `config::load` deliberately
/// swallows file-level problems (`Option`, not `Result`). This runs before the
/// first working-tree mutation, so rejecting here still leaves the repo
/// untouched.
fn resolve_conflict_config(args: &Cli) -> Result<ConflictConfig, GcmError> {
    // Capture CLI overrides (all Options / bool) so we know which fields the
    // user explicitly provided. Options take precedence over config.
    let cli = if let Some(Commands::Resolve {
        conflict_temperature,
        conflict_validate_cmd,
        conflict_auto_policy,
        conflict_sensitive_paths,
        no_mergiraf,
        max_rounds,
        no_finish: _,
        pr: _,
        mr: _,
        remote_push: _,
        remote_comment: _,
    }) = &args.command
    {
        Some(ConflictCli {
            temperature: *conflict_temperature,
            validate_cmd: conflict_validate_cmd.clone(),
            sensitive_paths: conflict_sensitive_paths.clone(),
            auto_policy: *conflict_auto_policy,
            no_mergiraf: *no_mergiraf,
            max_rounds: *max_rounds,
        })
    } else {
        None
    };

    let mut cfg = match &cli {
        Some(c) => ConflictConfig {
            temperature: c.temperature.unwrap_or(0.1),
            validate_cmd: c.validate_cmd.clone(),
            sensitive_paths: c.sensitive_paths.clone().unwrap_or_default(),
            auto_policy: c.auto_policy.unwrap_or(AutoPolicy::Trivial),
            mergiraf: !c.no_mergiraf,
            max_rounds: c
                .max_rounds
                .unwrap_or_else(|| ConflictConfig::default().max_rounds),
        },
        None => ConflictConfig::default(),
    };

    if let Some(loaded) = crate::config::load() {
        match &cli {
            Some(c) => {
                if c.temperature.is_none() {
                    cfg.temperature = loaded.conflict.temperature;
                }
                if c.validate_cmd.is_none() {
                    cfg.validate_cmd = loaded.conflict.validate_cmd.clone();
                }
                if c.sensitive_paths
                    .as_ref()
                    .map(|v| v.is_empty())
                    .unwrap_or(true)
                {
                    cfg.sensitive_paths = loaded.conflict.sensitive_paths.clone();
                }
                if c.auto_policy.is_none() {
                    cfg.auto_policy = loaded.conflict.auto_policy;
                }
                if c.no_mergiraf {
                    // Explicit --no-mergiraf disables; do not let config re-enable.
                } else {
                    cfg.mergiraf = loaded.conflict.mergiraf;
                }
                if c.max_rounds.is_none() {
                    cfg.max_rounds = loaded.conflict.max_rounds;
                }
            }
            None => {
                cfg.temperature = loaded.conflict.temperature;
                cfg.validate_cmd = loaded.conflict.validate_cmd.clone();
                cfg.sensitive_paths = loaded.conflict.sensitive_paths.clone();
                cfg.auto_policy = loaded.conflict.auto_policy;
                cfg.mergiraf = loaded.conflict.mergiraf;
                cfg.max_rounds = loaded.conflict.max_rounds;
            }
        }
    }

    // A config-supplied 0 never reached clap's range check.
    if cfg.max_rounds == 0 {
        return Err(GcmError::Config(format!(
            "config.toml: [conflict] {}",
            crate::config::MAX_ROUNDS_ZERO
        )));
    }

    Ok(cfg)
}

#[derive(Debug, Clone)]
struct ConflictCli {
    max_rounds: Option<u32>,
    temperature: Option<f64>,
    validate_cmd: Option<String>,
    sensitive_paths: Option<Vec<String>>,
    auto_policy: Option<AutoPolicy>,
    no_mergiraf: bool,
}

/// Build the proposal for one unmerged file (phase A). Runs mergiraf, the
/// hunk classifier, the provider, and the validation gate - but never writes
/// the resolution back, stages, or prompts; those belong to the confirm and
/// apply phases.
fn propose_file(
    repo: &Repo,
    path: &str,
    conflict: &ConflictConfig,
    binary_set: &HashSet<String>,
    provider: &dyn Provider,
    privacy: &Privacy,
    args: &Cli,
) -> Result<FileProposal, GcmError> {
    let escalated_file = |reason: &str| {
        eprintln!("gcm resolve: {reason}");
        FileProposal {
            path: path.to_string(),
            hunks_total: 0,
            hunks_auto: 0,
            hunks_llm: 0,
            hunks_escalated: 0,
            kind: ProposalKind::EscalatedFile,
        }
    };

    if binary_set.contains(path) {
        return Ok(escalated_file(&format!("skipping {path} (binary file)")));
    }

    if is_sensitive_path(path, &conflict.sensitive_paths) {
        return Ok(escalated_file(&format!(
            "escalating {path} (matches sensitive_paths)"
        )));
    }

    let content = repo.read_file(path)?;
    let file = parse(path.to_string(), &content);

    if file.hunks.is_empty() {
        // File was already resolved (e.g. by a prior run or by hand) - staged
        // as-is in the apply phase.
        return Ok(FileProposal {
            path: path.to_string(),
            hunks_total: 0,
            hunks_auto: 0,
            hunks_llm: 0,
            hunks_escalated: 0,
            kind: ProposalKind::AlreadyResolved,
        });
    }

    // Optional mergiraf pre-stage. Skip in dry-run to avoid mutating the
    // working tree. A full mergiraf resolution becomes an ordinary proposal:
    // it is previewed and confirmed like any LLM resolution, never silently
    // accepted (the snapshot keeps the in-place mutation reversible).
    if !args.dry_run && conflict.mergiraf && mergiraf::try_resolve(repo, path)? {
        let after = repo.read_file(path)?;
        let file = parse(path.to_string(), &after);
        if file.hunks.is_empty() {
            return Ok(FileProposal {
                path: path.to_string(),
                hunks_total: 0,
                hunks_auto: 0,
                hunks_llm: 0,
                hunks_escalated: 0,
                kind: ProposalKind::Resolved { text: after },
            });
        }
    }

    let total = file.hunks.len();
    let mut resolutions: Vec<Option<String>> = vec![None; total];
    let mut auto_count = 0;
    let mut llm_indices = Vec::new();

    for (i, hunk) in file.hunks.iter().enumerate() {
        let resolution = match conflict.auto_policy {
            AutoPolicy::Complex => HunkResolution::Complex,
            AutoPolicy::Trivial | AutoPolicy::Moderate => classify(hunk),
        };
        match resolution {
            HunkResolution::Auto { text, .. } => {
                resolutions[i] = Some(text);
                auto_count += 1;
            }
            HunkResolution::Complex => {
                llm_indices.push(i);
            }
        }
    }

    let mut llm_count = 0;
    let mut escalated_count = 0;

    if !llm_indices.is_empty() {
        // Privacy filter on hunk text before provider egress.
        // Abort mode: fail if secrets detected. Redact mode: transform hunk text.
        // Off mode: no filtering.
        let scan_mode = privacy.secret_scan_mode();

        // For Abort mode, pre-scan all hunks and fail before any provider call.
        if scan_mode == SecretScanMode::Abort {
            for i in &llm_indices {
                let h = &file.hunks[*i];
                let combined = format!("{}{}{}", h.base.as_deref().unwrap_or(""), h.ours, h.theirs);
                privacy.scan_text(combined)?;
            }
        }

        let provider_hunks: Vec<ConflictHunk> = llm_indices
            .iter()
            .map(|i| {
                let h = &file.hunks[*i];
                if scan_mode == SecretScanMode::Redact {
                    // Redact mode: transform hunk text to remove secrets.
                    let base = h
                        .base
                        .as_ref()
                        .map(|b| privacy.scan_text(b.clone()).unwrap_or_else(|_| b.clone()));
                    let ours = privacy
                        .scan_text(h.ours.clone())
                        .unwrap_or_else(|_| h.ours.clone());
                    let theirs = privacy
                        .scan_text(h.theirs.clone())
                        .unwrap_or_else(|_| h.theirs.clone());
                    ConflictHunk { base, ours, theirs }
                } else {
                    ConflictHunk {
                        base: h.base.clone(),
                        ours: h.ours.clone(),
                        theirs: h.theirs.clone(),
                    }
                }
            })
            .collect();

        let ctx = ResolveContext {
            path: path.to_string(),
            hunks: provider_hunks,
            style_context: prompt::extract_style_context(&file),
            temperature: conflict.temperature,
        };

        let budget = provider.diff_budget();
        let batches = batch_hunks(ctx, budget.total_bytes);
        let mut llm_results: Vec<Resolution> = Vec::new();
        let mut hunk_offset = 0;
        for batch in batches {
            let num_hunks = batch.hunks.len();
            match provider.resolve_hunks(&batch) {
                Ok(mut batch_results) => {
                    for r in &mut batch_results {
                        r.hunk_index += hunk_offset;
                    }
                    llm_results.append(&mut batch_results);
                }
                Err(e) => {
                    // A provider failure is a tool escalation, not a run
                    // abort (owner decision 1): the file keeps its markers,
                    // other files proceed, and the run reports Partial. The
                    // actionable provider error still reaches the user here.
                    eprintln!("gcm resolve: {path}: provider error - {e}; escalating this file");
                    break;
                }
            }
            hunk_offset += num_hunks;
        }

        // Map back to original hunk indices (batch hunks are in 0..N order).
        for r in llm_results {
            if r.hunk_index < llm_indices.len() {
                let original = llm_indices[r.hunk_index];
                resolutions[original] = Some(r.replacement);
            }
        }

        for i in &llm_indices {
            if resolutions[*i].is_some() {
                llm_count += 1;
            } else {
                escalated_count += 1;
            }
        }
    }

    // Reconstruct the resolved file text.
    let resolved_text = reconstruct(&file, &resolutions, &content);

    // If at least one hunk could not be resolved, keep the original conflict
    // marker block(s) in place and report the file as escalated. Do not run the
    // validation gate here: retained markers are the expected escalation
    // artifact, not a provider-output validation failure.
    if escalated_count > 0 {
        if args.dry_run && !args.json {
            eprintln!(
                "gcm resolve: {path} would be partially resolved ({auto_count} auto, {llm_count} LLM, {escalated_count} escalated)"
            );
        }
        return Ok(FileProposal {
            path: path.to_string(),
            hunks_total: total,
            hunks_auto: auto_count,
            hunks_llm: llm_count,
            hunks_escalated: escalated_count,
            kind: ProposalKind::EscalatedHunks,
        });
    }

    // Validation gate. One bounded retry asks the provider to fix its own
    // output; a retry that still fails escalates the file (AC5) - marker
    // retention or a failing validate_cmd is a tool limit, never a run abort.
    let validated_text = match validate(
        &resolved_text,
        conflict.validate_cmd.as_deref(),
        repo,
        path,
    ) {
        Ok(()) => resolved_text,
        Err(first_failure) => {
            match attempt_validation_retry(
                provider,
                &file,
                &resolutions,
                &content,
                conflict.temperature,
                repo,
                path,
            ) {
                Ok(retried) => retried,
                Err(retry_err) => {
                    eprintln!(
                            "gcm resolve: {path}: validation failed ({first_failure:?}), retry failed ({retry_err}); escalating this file"
                        );
                    escalated_count += llm_count;
                    return Ok(FileProposal {
                        path: path.to_string(),
                        hunks_total: total,
                        hunks_auto: auto_count,
                        hunks_llm: 0,
                        hunks_escalated: escalated_count,
                        kind: ProposalKind::EscalatedHunks,
                    });
                }
            }
        }
    };

    if args.dry_run && !args.json {
        eprintln!("gcm resolve: {path} would be resolved ({auto_count} auto, {llm_count} LLM)");
    }

    Ok(FileProposal {
        path: path.to_string(),
        hunks_total: total,
        hunks_auto: auto_count,
        hunks_llm: llm_count,
        hunks_escalated: escalated_count,
        kind: ProposalKind::Resolved {
            text: validated_text,
        },
    })
}

// Privacy::secret_scan is now public via Privacy::secret_scan_mode.

fn batch_hunks(ctx: ResolveContext, total_budget: usize) -> Vec<ResolveContext> {
    if ctx.hunks.is_empty() {
        return vec![ctx];
    }
    // Leave 25% headroom for system prompt, schema, and style context.
    let effective = (total_budget as f64 * 0.75) as usize;
    let mut batches = Vec::new();
    let mut current_hunks = Vec::new();
    let mut current_size = 0usize;
    for h in ctx.hunks {
        let size = h.ours.len() + h.theirs.len() + h.base.as_ref().map_or(0, String::len);
        if !current_hunks.is_empty() && current_size + size > effective {
            batches.push(ResolveContext {
                path: ctx.path.clone(),
                hunks: std::mem::take(&mut current_hunks),
                style_context: ctx.style_context.clone(),
                temperature: ctx.temperature,
            });
            current_size = 0;
        }
        current_size += size;
        current_hunks.push(h);
    }
    if !current_hunks.is_empty() {
        batches.push(ResolveContext {
            path: ctx.path.clone(),
            hunks: current_hunks,
            style_context: ctx.style_context,
            temperature: ctx.temperature,
        });
    }
    batches
}

fn attempt_validation_retry(
    provider: &dyn Provider,
    file: &ConflictFile,
    resolutions: &[Option<String>],
    content: &str,
    temperature: f64,
    repo: &Repo,
    path: &str,
) -> Result<String, GcmError> {
    let mut retry_hunks = Vec::new();
    let mut retry_indices = Vec::new();
    for (i, h) in file.hunks.iter().enumerate() {
        if let Some(text) = &resolutions[i] {
            if has_conflict_markers(text) {
                retry_hunks.push(ConflictHunk {
                    base: h.base.clone(),
                    ours: text.clone(),
                    theirs: h.theirs.clone(),
                });
                retry_indices.push(i);
            }
        }
    }
    if retry_hunks.is_empty() {
        return Err(GcmError::ResolutionEscalated {
            path: path.to_string(),
            reason: "validation retry found no markers to fix".to_string(),
        });
    }
    let ctx = ResolveContext {
        path: path.to_string(),
        hunks: retry_hunks,
        style_context: prompt::extract_style_context(file),
        temperature,
    };
    let fixed = provider.resolve_hunks(&ctx)?;
    let mut new_resolutions = resolutions.to_vec();
    for r in fixed {
        if r.hunk_index < retry_indices.len() {
            new_resolutions[retry_indices[r.hunk_index]] = Some(r.replacement);
        }
    }
    let text = reconstruct(file, &new_resolutions, content);
    if has_conflict_markers(&text) {
        return Err(GcmError::ResolutionEscalated {
            path: path.to_string(),
            reason: "retry still produced conflict markers".to_string(),
        });
    }
    validate(&text, None, repo, path).map_err(|e| GcmError::ResolutionEscalated {
        path: path.to_string(),
        reason: format!("retry validation failed: {e:?}"),
    })?;
    Ok(text)
}

fn reconstruct(file: &ConflictFile, resolutions: &[Option<String>], original: &str) -> String {
    let original_lines: Vec<&str> = original.lines().collect();
    // Detect dominant line ending to preserve CRLF files.
    let uses_crlf = original.contains("\r\n");
    let mut out = String::new();
    let mut hunk_idx = 0;
    let mut line_no = 1usize;
    while line_no <= original_lines.len() {
        if hunk_idx < file.hunks.len() && line_no == file.hunks[hunk_idx].start_line {
            if let Some(text) = &resolutions[hunk_idx] {
                // Normalize resolution text line endings to match the file.
                if uses_crlf && !text.contains("\r\n") {
                    // Convert LF to CRLF in the resolution text.
                    let normalized = text.replace('\n', "\r\n");
                    out.push_str(&normalized);
                } else {
                    out.push_str(text);
                }
                // Guard: a resolution without a trailing newline must not fuse with the
                // following context line. Append exactly one line ending if missing.
                if !text.is_empty() {
                    if uses_crlf {
                        if !out.ends_with("\r\n") {
                            out.push_str("\r\n");
                        }
                    } else if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
            } else {
                // Escalated: keep the original hunk block verbatim.
                for l in line_no..=file.hunks[hunk_idx].end_line {
                    if l - 1 < original_lines.len() {
                        out.push_str(original_lines[l - 1]);
                        out.push('\n');
                    }
                }
            }
            line_no = file.hunks[hunk_idx].end_line + 1;
            hunk_idx += 1;
        } else {
            out.push_str(original_lines[line_no - 1]);
            out.push('\n');
            line_no += 1;
        }
    }
    // Preserve a trailing newline only if the original had one.
    if !original.ends_with('\n') && !out.is_empty() {
        out.pop();
        // For CRLF files, the pop above removes only the LF; remove any dangling CR too.
        if uses_crlf && out.ends_with('\r') {
            out.pop();
        }
    }
    out
}

fn is_sensitive_path(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| glob_match(p, path))
}

fn glob_match(pattern: &str, path: &str) -> bool {
    // Minimal glob support: * matches any sequence, ? matches one char.
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = path.chars().collect();
    let mut dp = vec![vec![false; txt.len() + 1]; pat.len() + 1];
    dp[0][0] = true;
    for i in 1..=pat.len() {
        if pat[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=pat.len() {
        for j in 1..=txt.len() {
            dp[i][j] = match pat[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                c => c == txt[j - 1] && dp[i - 1][j - 1],
            };
        }
    }
    dp[pat.len()][txt.len()]
}

/// Rounds that put a commit on the branch: both a completed finish and a
/// stop-on-next-conflict committed the step they were given. A round whose
/// finish was skipped or never attempted (an escalation, or the round the user
/// rejected) committed nothing.
fn committed_rounds(lr: &report::LoopReport) -> usize {
    lr.rounds
        .iter()
        .filter(|r| {
            matches!(
                r.finish.as_ref().map(|f| f.result),
                Some(FinishResult::Completed) | Some(FinishResult::StoppedOnConflict)
            )
        })
        .count()
}

/// The sequencing operation the loop was driving. Only rebase and cherry-pick
/// reach the loop, and every round that finished names its op, so the scan
/// finds one unless the very first round was rejected before finishing.
fn loop_op(lr: &report::LoopReport) -> &str {
    lr.rounds
        .iter()
        .rev()
        .find_map(|r| r.finish.as_ref().and_then(|f| f.op.as_deref()))
        .unwrap_or("rebase")
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// "Round 1 stays committed." / "Rounds 1-3 stay committed." - the rounds a
/// mid-loop stop does NOT undo, named so the user knows what is at stake before
/// reaching for `--abort`.
fn kept_rounds_line(kept: usize) -> String {
    if kept == 1 {
        "Round 1 stays committed.".to_string()
    } else {
        format!("Rounds 1-{kept} stay committed.")
    }
}

/// The exact ways out of a still-stopped sequence. Naming `--abort` without
/// naming what it destroys would be the dangerous half of the truth: it
/// discards the rounds already committed, not just the current stop.
fn print_recovery(lr: &report::LoopReport, op: &str) {
    match lr.stopped_on.as_deref() {
        Some(sha) => println!("The {op} is still stopped on {sha}."),
        None => println!("The {op} is still stopped."),
    }
    println!(
        "Re-run 'gcm resolve' to continue, or resolve by hand and 'git add' then 'git {op} --continue'."
    );
    let kept = committed_rounds(lr);
    if kept > 0 {
        println!(
            "'git {op} --abort' would discard the {kept} round{} already committed.",
            plural(kept)
        );
    }
}

/// Headlines for a run the loop driver actually engaged on. A single-round run
/// carries no loop block and keeps the pre-CLO-554 wording verbatim.
fn print_loop_headline(report: &ResolveReport, lr: &report::LoopReport) {
    let op = loop_op(lr);
    let n = lr.rounds_run;
    match lr.terminal {
        report::LoopTerminal::Completed => {
            let sha = report
                .finish
                .as_ref()
                .and_then(|f| f.commit.as_deref())
                .unwrap_or("HEAD");
            println!("All conflicts resolved - {op} completed ({sha}) across {n} rounds.");
        }
        report::LoopTerminal::CapReached => {
            // Keeps the legacy substring: with --max-rounds 1 this is exactly
            // the situation the pre-loop build reported.
            println!(
                "All conflicts resolved here - the {op} continued and stopped on the next conflicted commit."
            );
            println!(
                "Round cap reached ({n} of {}). Raise --max-rounds to go further in one run.",
                lr.max_rounds
            );
            print_recovery(lr, op);
        }
        report::LoopTerminal::Declined => {
            println!(
                "Stopped after {n} round{} at your request - nothing was spent on the next one.",
                plural(n)
            );
            print_recovery(lr, op);
        }
        report::LoopTerminal::Aborted => {
            let kept = committed_rounds(lr);
            println!("Aborted - round {n} restored, nothing changed in it.");
            if kept > 0 {
                println!("{}", kept_rounds_line(kept));
            }
            print_recovery(lr, op);
        }
        report::LoopTerminal::NoProgress => {
            let where_ = lr.stopped_on.as_deref().unwrap_or("its current commit");
            println!(
                "Stopped after {n} round{} - the {op} did not advance past {where_}.",
                plural(n)
            );
            print_recovery(lr, op);
        }
        report::LoopTerminal::Partial => {
            let kept = committed_rounds(lr);
            println!("Some files resolved; others were skipped or escalated.");
            if kept > 0 {
                println!("{}", kept_rounds_line(kept));
            }
        }
    }
}

fn print_human_report(report: &ResolveReport) {
    if let Some(lr) = report.loop_report.as_ref() {
        print_loop_headline(report, lr);
        print_file_lines(report);
        // The escalation trailer still applies: a Partial loop leaves the same
        // unmerged paths a Partial single round would.
        print_escalation_trailer(report);
        return;
    }
    print_single_round_headline(report);
    print_file_lines(report);
    print_escalation_trailer(report);
}

fn print_single_round_headline(report: &ResolveReport) {
    let finish = report.finish.as_ref();
    match &report.status {
        ResolveStatus::Resolved => match finish {
            Some(f) if f.result == FinishResult::Completed => {
                let sha = f.commit.as_deref().unwrap_or("HEAD");
                match f.op.as_deref() {
                    Some("merge") => println!("All conflicts resolved - merge committed ({sha})."),
                    Some(op) => println!("All conflicts resolved - {op} completed ({sha})."),
                    None => println!("All conflicts resolved - committed ({sha})."),
                }
            }
            Some(f) if f.result == FinishResult::StoppedOnConflict => {
                let op = f.op.as_deref().unwrap_or("rebase");
                println!(
                    "All conflicts resolved here - the {op} continued and stopped on the next conflicted commit. Run 'gcm resolve' again."
                );
            }
            Some(f) if f.result == FinishResult::Skipped => match f.op.as_deref() {
                Some(op) => println!(
                    "All conflicts resolved and staged. Run 'git {op} --continue' to finish."
                ),
                None => println!(
                    "All conflicts resolved and staged (no merge, rebase, or cherry-pick in progress to finish)."
                ),
            },
            _ => println!("All conflicts resolved."),
        },
        ResolveStatus::Partial => {
            println!("Some files resolved; others were skipped or escalated.");
        }
        ResolveStatus::Noop => println!("No conflicts to resolve."),
        ResolveStatus::Aborted => println!("Aborted - working tree restored, nothing changed."),
        ResolveStatus::Error => println!("Resolution failed."),
    }
}

fn print_file_lines(report: &ResolveReport) {
    for f in &report.files {
        println!(
            "  {}: {} total, {} auto, {} LLM, {} escalated ({:?})",
            f.path, f.hunks_total, f.hunks_auto, f.hunks_llm, f.hunks_escalated, f.action
        );
    }
}

/// Escalation trailer (local runs only - `finish` is set only there): name
/// what remains and the exact way out.
fn print_escalation_trailer(report: &ResolveReport) {
    let finish = report.finish.as_ref();
    if report.status == ResolveStatus::Partial && finish.is_some() {
        let remaining: Vec<&str> = report
            .files
            .iter()
            .filter(|f| matches!(f.action, FileAction::Escalated | FileAction::Skipped))
            .map(|f| f.path.as_str())
            .collect();
        if !remaining.is_empty() {
            println!("Still conflicted: {}", remaining.join(", "));
            let cmd = finish
                .and_then(|f| f.op.as_deref())
                .map(|op| format!("git {op} --continue"))
                .unwrap_or_else(|| "git merge/rebase/cherry-pick --continue".to_string());
            println!(
                "Re-run 'gcm resolve' (or resolve by hand and 'git add'), then finish with {cmd}."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(op: &'static str, sha: &str) -> OpHead {
        (op, sha.to_string())
    }

    #[test]
    fn advanced_detects_a_moving_sequence() {
        let a = head("rebase", "a1b2c3d");
        let b = head("rebase", "b2c3d4e");
        assert!(advanced(Some(&a), Some(&b)), "a new commit means progress");
    }

    #[test]
    fn advanced_rejects_a_stalled_sequence() {
        let a = head("rebase", "a1b2c3d");
        let same = head("rebase", "a1b2c3d");
        assert!(
            !advanced(Some(&a), Some(&same)),
            "the same commit twice is a stall, not a round worth spending on"
        );
    }

    #[test]
    fn advanced_rejects_a_vanished_head() {
        // The finish claimed StoppedOnConflict, so a sequencing head must
        // exist. Its absence means the postcondition classification and the
        // repo disagree - stop rather than propose blind.
        let a = head("cherry-pick", "a1b2c3d");
        assert!(!advanced(Some(&a), None));
        assert!(!advanced(None, None));
    }

    #[test]
    fn advanced_accepts_the_first_sequencing_head() {
        // Round 1 of a merge-then-rebase mix: no previous head, now there is
        // one. That is forward motion.
        let b = head("rebase", "b2c3d4e");
        assert!(advanced(None, Some(&b)));
    }

    #[test]
    fn snapshot_restore_is_byte_exact() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repo::at_root(dir.path().to_path_buf());
        let crlf =
            b"line 1\r\n<<<<<<< HEAD\r\nours\r\n=======\r\ntheirs\r\n>>>>>>> f\r\nline 2\r\n";
        let no_newline = b"partial manual resolution without trailing newline";
        std::fs::write(dir.path().join("a.txt"), crlf).unwrap();
        std::fs::write(dir.path().join("b.txt"), no_newline).unwrap();

        let mut snap =
            WorkingTreeSnapshot::capture(&repo, &["a.txt".to_string(), "b.txt".to_string()])
                .unwrap();
        repo.write_file("a.txt", "zdiff3 rewritten\n").unwrap();
        repo.write_file("b.txt", "mergiraf rewritten\n").unwrap();
        snap.record_written(&repo).unwrap();

        snap.restore(&repo).unwrap();
        assert_eq!(repo.read_file_bytes("a.txt").unwrap(), crlf.to_vec());
        assert_eq!(repo.read_file_bytes("b.txt").unwrap(), no_newline.to_vec());
    }

    #[test]
    fn snapshot_restore_skips_externally_modified_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repo::at_root(dir.path().to_path_buf());
        std::fs::write(dir.path().join("a.txt"), b"original a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"original b").unwrap();

        let mut snap =
            WorkingTreeSnapshot::capture(&repo, &["a.txt".to_string(), "b.txt".to_string()])
                .unwrap();
        repo.write_file("a.txt", "gcm wrote a").unwrap();
        repo.write_file("b.txt", "gcm wrote b").unwrap();
        snap.record_written(&repo).unwrap();

        // Another terminal edits a.txt mid-run: the guard must not clobber it.
        repo.write_file("a.txt", "external edit").unwrap();

        snap.restore(&repo).unwrap();
        assert_eq!(repo.read_file_bytes("a.txt").unwrap(), b"external edit");
        assert_eq!(repo.read_file_bytes("b.txt").unwrap(), b"original b");
    }

    #[test]
    fn snapshot_restore_skips_externally_deleted_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repo::at_root(dir.path().to_path_buf());
        std::fs::write(dir.path().join("a.txt"), b"original a").unwrap();

        let mut snap = WorkingTreeSnapshot::capture(&repo, &["a.txt".to_string()]).unwrap();
        repo.write_file("a.txt", "gcm wrote a").unwrap();
        snap.record_written(&repo).unwrap();
        std::fs::remove_file(dir.path().join("a.txt")).unwrap();

        snap.restore(&repo).unwrap();
        assert!(
            !dir.path().join("a.txt").exists(),
            "externally deleted file must stay deleted"
        );
    }

    #[test]
    fn glob_match_basic() {
        assert!(glob_match("*.rs", "src/lib.rs"));
        assert!(glob_match("secrets/*", "secrets/key.pem"));
        assert!(!glob_match("secrets/*", "src/secrets/key.pem"));
        assert!(glob_match("?.*", "a.rs"));
    }

    #[test]
    fn is_sensitive_path_matches() {
        assert!(is_sensitive_path(
            "secrets/key.pem",
            &["secrets/*".to_string()]
        ));
        assert!(!is_sensitive_path("src/lib.rs", &["secrets/*".to_string()]));
    }

    #[test]
    fn batch_hunks_empty_returns_single() {
        let ctx = ResolveContext {
            path: "f.txt".to_string(),
            hunks: vec![],
            style_context: String::new(),
            temperature: 0.1,
        };
        assert_eq!(batch_hunks(ctx, 1000).len(), 1);
    }

    #[test]
    fn reconstruct_resolution_missing_newline_keeps_following_line() {
        let content = "line 1\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nline 2\n";
        let file = parse("f.txt".to_string(), content);
        let resolutions: Vec<Option<String>> = vec![Some("resolved".to_string())];
        let out = reconstruct(&file, &resolutions, content);
        assert!(!out.contains("<<<<<<<"));
        assert!(
            out.contains("resolved\nline 2"),
            "context line should stay separate: {out:?}"
        );
        assert!(
            !out.contains("resolvedline 2"),
            "resolution fused with context: {out:?}"
        );
    }

    #[test]
    fn reconstruct_resolution_with_newline_no_double_blank() {
        let content = "line 1\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nline 2\n";
        let file = parse("f.txt".to_string(), content);
        let resolutions: Vec<Option<String>> = vec![Some("resolved\n".to_string())];
        let out = reconstruct(&file, &resolutions, content);
        assert!(
            !out.contains("resolved\n\nline 2"),
            "guard added a second newline: {out:?}"
        );
    }

    #[test]
    fn reconstruct_crlf_resolution_missing_newline() {
        let content =
            "line 1\r\n<<<<<<< HEAD\r\nours\r\n=======\r\ntheirs\r\n>>>>>>> feature\r\nline 2\r\n";
        let file = parse("f.txt".to_string(), content);
        let resolutions: Vec<Option<String>> = vec![Some("resolved".to_string())];
        let out = reconstruct(&file, &resolutions, content);
        assert!(!out.contains("<<<<<<<"));
        assert!(
            out.contains("resolved\r\nline 2"),
            "context line should stay separate: {out:?}"
        );
        assert!(
            !out.contains("resolvedline 2"),
            "resolution fused with context: {out:?}"
        );
    }

    #[test]
    fn reconstruct_crlf_no_final_newline_preserved() {
        let content = "<<<<<<< HEAD\r\nours\r\n=======\r\ntheirs\r\n>>>>>>> feature";
        let file = parse("f.txt".to_string(), content);
        let resolutions: Vec<Option<String>> = vec![Some("resolved".to_string())];
        let out = reconstruct(&file, &resolutions, content);
        assert!(
            !out.ends_with("\r\n"),
            "CRLF file without final newline should stay trim: {out:?}"
        );
        assert!(!out.ends_with('\n'), "no dangling LF either: {out:?}");
        assert!(!out.ends_with('\r'), "no dangling CR either: {out:?}");
        assert_eq!(out, "resolved");
    }

    #[test]
    fn reconstruct_empty_resolution_no_extra_blank() {
        let content = "line 1\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nline 2\n";
        let file = parse("f.txt".to_string(), content);
        let resolutions: Vec<Option<String>> = vec![Some("".to_string())];
        let out = reconstruct(&file, &resolutions, content);
        assert!(
            !out.contains("\n\n"),
            "empty resolution should not add a blank line: {out:?}"
        );
        assert!(
            out.contains("line 1\nline 2"),
            "context lines should abut: {out:?}"
        );
    }

    #[test]
    fn reconstruct_no_final_newline_preserved() {
        let content = "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature";
        let file = parse("f.txt".to_string(), content);
        let resolutions: Vec<Option<String>> = vec![Some("resolved".to_string())];
        let out = reconstruct(&file, &resolutions, content);
        assert!(
            !out.ends_with('\n'),
            "file without final newline should stay trim: {out:?}"
        );
    }

    #[test]
    fn reconstruct_replaces_hunk_with_resolution() {
        let content = "line 1\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nline 2\n";
        let file = parse("f.txt".to_string(), content);
        let resolutions: Vec<Option<String>> = vec![Some("resolved\n".to_string())];
        let out = reconstruct(&file, &resolutions, content);
        assert!(!out.contains("<<<<<<<"));
        assert!(out.contains("resolved"));
        assert!(out.contains("line 1"));
        assert!(out.contains("line 2"));
    }
}
