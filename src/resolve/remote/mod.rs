//! Remote MR/PR orchestration for `gcm resolve` (CLO-533).
//!
//! This module is a thin wrapper around the Phase-1 local resolution engine.
//! It detects the host, verifies the required CLI is on PATH, builds an
//! isolated scratch clone, checks out the source and target branches, runs
//! the merge, and drives `resolve::run_resolve_in_repo` over the result.
//!
//! After resolution, the resolved tree is committed to the
//! `gcm-resolve-<host>-<number>` branch. Optional push and comment are opt-in.

pub mod fetch;
pub mod host;
pub mod publish;

use fetch::prepare_scratch_repo;
use host::{require_host_cli, resolve_remote_ref, Host, RemoteRef};

use crate::cli::Cli;
use crate::error::GcmError;
use crate::git::Repo;
use crate::resolve::report::{RemoteReport, ResolveReport, ResolveStatus};
use crate::resolve::run_resolve_in_repo;

/// Run the remote MR/PR resolution flow for the `resolve` subcommand.
///
/// * `current_repo` is the user's local repo (used for bare-id origin lookup
///   and to guarantee we never mutate it).
/// * `args` is the parsed CLI.
///
/// This function is intentionally shallow: it validates the request, builds a
/// scratch repo, sets up the merge state, and hands off to the Phase-1 engine.
/// All LLM resolution, validation, preview, and `--dry-run` semantics live in
/// `run_resolve_in_repo`.
#[allow(dead_code)]
pub fn run_resolve_remote(current_repo: &Repo, args: &Cli) -> Result<ResolveReport, GcmError> {
    run_resolve_remote_opt(Some(current_repo), args)
}

/// Run the remote MR/PR resolution flow, accepting an optional repo.
/// When `current_repo` is `None`, only full URLs work (bare ids need an origin).
pub fn run_resolve_remote_opt(
    current_repo: Option<&Repo>,
    args: &Cli,
) -> Result<ResolveReport, GcmError> {
    let (raw_arg, host) = extract_remote_arg(args)?;
    let remote_ref = resolve_remote_ref(&raw_arg, Some(host), current_repo)?;

    if args.dry_run {
        // No temp dir, no clone, no host CLI invocation. Produce a preview report.
        return Ok(dry_run_report(&remote_ref));
    }

    require_host_cli(remote_ref.host)?;

    let scratch = prepare_scratch_repo(&remote_ref)?;
    let resolution_branch = format!(
        "gcm-resolve-{}-{}",
        remote_ref.host.resolution_slug(),
        remote_ref.number
    );

    ensure_scratch_identity(&scratch.repo)?;

    // Create the resolution branch from the base and merge the source branch in.
    scratch
        .repo
        .run_git(&["checkout", "-B", &resolution_branch, &scratch.base_branch])?;
    let merge_result =
        scratch
            .repo
            .run_git(&["merge", "--no-ff", "--no-commit", &scratch.source_branch]);

    // Even a clean merge leaves the tree merged but no conflict state. A
    // conflicted merge sets up conflict state. Either way, run the core engine
    // with allow_no_conflict_state=true so a clean merge is reported as success.
    //
    // On error, the scratch `TempDir` is dropped automatically, cleaning up
    // (AC13: cleanup on error/abort). On success, we call `into_path()` to
    // preserve the scratch and report its path (AC7: print scratch path).
    let report = match merge_result {
        Ok(()) => run_resolve_in_repo(&scratch.repo, args, crate::resolve::ResolveMode::Remote),
        Err(e) => {
            // If merge failed because of conflicts, the engine will find unmerged
            // files and resolve them. Any other failure is propagated (TempDir
            // auto-deletes on the error path).
            let unmerged = scratch.repo.unmerged_files()?;
            if unmerged.is_empty() {
                return Err(e);
            }
            run_resolve_in_repo(&scratch.repo, args, crate::resolve::ResolveMode::Remote)
        }
    };

    let mut report = report?;

    // Stage the resolved tree and create the merge commit on the resolution
    // branch. This persists the resolution so it survives after the scratch
    // is preserved and so --remote-push pushes the correct tree.
    commit_resolution(&scratch.repo, &resolution_branch, &remote_ref)?;

    // Optional publish: push and/or comment. Comment failures are surfaced
    // as warnings but do not abort the resolution (EC7).
    let mut pushed = false;
    let mut commented = false;
    let mut comment_warning: Option<String> = None;

    if args.remote_push() {
        fetch::push_resolution_branch(&scratch.repo, &resolution_branch, remote_ref.host)?;
        pushed = true;
    }

    if args.remote_comment() {
        match publish::post_comment(&scratch.repo, &remote_ref, &report) {
            Ok(()) => commented = true,
            Err(e) => {
                // EC7: surface but do not abort.
                comment_warning = Some(e.to_string());
                eprintln!("gcm resolve: warning: comment failed: {e}");
            }
        }
    }

    // Capture branch names before consuming scratch.
    let scratch_base_branch = scratch.base_branch.clone();
    let scratch_source_branch = scratch.source_branch.clone();

    // Preserve the scratch repo on success (AC7) by consuming the TempDir.
    let scratch_path = scratch.into_path();
    let scratch_path_str = scratch_path.to_string_lossy().to_string();

    // Always attach RemoteReport so the caller can print metadata.
    report.remote = Some(RemoteReport {
        host: remote_ref.host,
        number: remote_ref.number,
        base_branch: scratch_base_branch,
        source_branch: scratch_source_branch,
        resolution_branch: resolution_branch.clone(),
        pushed,
        commented,
        scratch_path: Some(scratch_path_str),
    });

    // If comment failed, downgrade status to Partial so the user knows not
    // everything succeeded. (The resolution itself is still committed.)
    if comment_warning.is_some() && report.status == ResolveStatus::Resolved {
        report.status = ResolveStatus::Partial;
    }

    Ok(report)
}

/// Configure a local fallback identity for scratch-repo merge/commit operations.
fn ensure_scratch_identity(repo: &Repo) -> Result<(), GcmError> {
    repo.run_git(&["config", "user.email", "gcm@resolve.local"])?;
    repo.run_git(&["config", "user.name", "gcm resolve"])?;
    Ok(())
}

/// Stage all changes and create the merge commit on the resolution branch.
fn commit_resolution(repo: &Repo, branch: &str, remote_ref: &RemoteRef) -> Result<(), GcmError> {
    // Stage all changes (resolved files + the merged tree).
    repo.run_git(&["add", "-A"])?;

    // Create the commit. Use --no-verify to skip hooks that might interfere
    // in the scratch repo. Use --allow-empty so a clean merge (no changes
    // beyond what merge --no-ff already staged) still commits.
    let msg = format!(
        "gcm resolve: merge {} into {} ({} #{})",
        remote_ref.owner, // Simplified message
        branch,
        remote_ref.host.cli_name(),
        remote_ref.number
    );
    repo.run_git(&["commit", "--no-verify", "--allow-empty", "-m", &msg])?;

    Ok(())
}

fn extract_remote_arg(args: &Cli) -> Result<(String, Host), GcmError> {
    if let Some(crate::cli::Commands::Resolve { pr: Some(p), .. }) = &args.command {
        return Ok((p.clone(), Host::GitHub));
    }
    if let Some(crate::cli::Commands::Resolve { mr: Some(m), .. }) = &args.command {
        return Ok((m.clone(), Host::GitLab));
    }
    Err(GcmError::RemoteHost {
        host: "unknown".to_string(),
        reason: "remote resolve requires --pr or --mr".to_string(),
    })
}

fn dry_run_report(remote_ref: &RemoteRef) -> ResolveReport {
    ResolveReport {
        v: crate::output::SCHEMA_VERSION,
        status: ResolveStatus::Noop,
        files: vec![],
        staged: vec![],
        finish: None,
        restored: false,
        remote: Some(RemoteReport {
            host: remote_ref.host,
            number: remote_ref.number,
            base_branch: String::new(),
            source_branch: String::new(),
            resolution_branch: format!(
                "gcm-resolve-{}-{}",
                remote_ref.host.resolution_slug(),
                remote_ref.number
            ),
            pushed: false,
            commented: false,
            scratch_path: None,
        }),
    }
}
