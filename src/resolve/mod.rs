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
use crate::privacy::{Privacy, SecretScanMode};
use crate::provider::{ConflictHunk, Provider, Resolution, ResolveContext};

use crate::git::FinishOutcome;
use classify::{classify, HunkResolution};
use markers::{has_conflict_markers, parse, ConflictFile};
use report::{FileAction, FileReport, FinishReport, FinishResult, ResolveReport, ResolveStatus};
use validate::{validate, ValidationError};

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
    let report = run_resolve_in_repo(&repo, args, ResolveMode::Local)?;
    if args.json {
        report::emit(&report);
    } else {
        print_human_report(&report);
    }
    Ok(())
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

    let conflict = resolve_conflict_config(args);

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

    let provider = crate::provider::select(args.provider, args.model.as_deref())
        .map_err(GcmError::Provider)?;
    let privacy = Privacy::load(repo, args.secret_scan)?;

    // Non-interactive guard: if we would need to prompt but can't, error early.
    if crate::ui::needs_terminal_but_absent(args.yes, args.dry_run) {
        return Err(GcmError::NonInteractive);
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
                validate(&edited, conflict.validate_cmd.as_deref(), repo, &path).map_err(|e| {
                    GcmError::ResolutionEscalated {
                        path: path.clone(),
                        reason: format!("edited content failed validation: {e:?}"),
                    }
                })?;
                texts[i] = Some(edited);
                decisions[i] = Some(FileAction::Edited);
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
    // edited, mergiraf-resolved, and already-marker-free files alike.
    let mut files = Vec::with_capacity(proposals.len());
    let mut staged: Vec<String> = Vec::new();
    for (i, p) in proposals.iter().enumerate() {
        let action = match &p.kind {
            ProposalKind::Resolved { .. } => {
                let action = decisions[i].expect("every resolved proposal was decided");
                let text = texts[i].as_ref().expect("resolved text present");
                repo.write_file(&p.path, text)?;
                staged.push(p.path.clone());
                action
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
    if !staged.is_empty() {
        let refs: Vec<&str> = staged.iter().map(String::as_str).collect();
        repo.stage_paths(&refs)?;
    }

    let mut report = report_for(files);
    report.staged = staged;

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
    }
}

fn resolve_conflict_config(args: &Cli) -> ConflictConfig {
    // Capture CLI overrides (all Options / bool) so we know which fields the
    // user explicitly provided. Options take precedence over config.
    let cli = if let Some(Commands::Resolve {
        conflict_temperature,
        conflict_validate_cmd,
        conflict_auto_policy,
        conflict_sensitive_paths,
        no_mergiraf,
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
            }
            None => {
                cfg.temperature = loaded.conflict.temperature;
                cfg.validate_cmd = loaded.conflict.validate_cmd.clone();
                cfg.sensitive_paths = loaded.conflict.sensitive_paths.clone();
                cfg.auto_policy = loaded.conflict.auto_policy;
                cfg.mergiraf = loaded.conflict.mergiraf;
            }
        }
    }

    cfg
}

#[derive(Debug, Clone)]
struct ConflictCli {
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
            let mut batch_results = provider.resolve_hunks(&batch)?;
            for r in &mut batch_results {
                r.hunk_index += hunk_offset;
            }
            llm_results.append(&mut batch_results);
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

    // Validation gate.
    let validated_text =
        match validate(&resolved_text, conflict.validate_cmd.as_deref(), repo, path) {
            Ok(()) => resolved_text,
            Err(ValidationError::ConflictMarkers) => {
                // One bounded retry: ask the provider to fix its own output.
                attempt_validation_retry(
                    provider,
                    &file,
                    &resolutions,
                    &content,
                    conflict.temperature,
                    repo,
                    path,
                )?
            }
            Err(ValidationError::ValidateCmdFailed { .. }) => {
                // One bounded retry: ask the provider to fix its own output.
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
                    Err(_) => {
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

fn print_human_report(report: &ResolveReport) {
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
    for f in &report.files {
        println!(
            "  {}: {} total, {} auto, {} LLM, {} escalated ({:?})",
            f.path, f.hunks_total, f.hunks_auto, f.hunks_llm, f.hunks_escalated, f.action
        );
    }
    // Escalation trailer (local runs only - `finish` is set only there): name
    // what remains and the exact way out.
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
