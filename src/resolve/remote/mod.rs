//! Remote MR/PR orchestration for `gcm resolve` (CLO-533).
//!
//! This module is a thin wrapper around the Phase-1 local resolution engine.
//! It detects the host, verifies the required CLI is on PATH, builds an
//! isolated scratch clone, checks out the source and target branches, runs
//! the merge, and drives `resolve::run_resolve_in_repo` over the result.

pub mod host;

use crate::cli::Cli;
use crate::error::GcmError;
use crate::git::Repo;
use crate::resolve::report::ResolveReport;
use host::{require_host_cli, resolve_remote_ref, Host, RemoteRef};

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
    let (raw_arg, host) = extract_remote_arg(args)?;
    let remote_ref = resolve_remote_ref(&raw_arg, Some(host), Some(current_repo))?;
    require_host_cli(remote_ref.host)?;

    if args.dry_run {
        // No temp dir, no clone, no host CLI invocation. Produce a preview report.
        return Ok(dry_run_report(&remote_ref));
    }

    // TODO: build scratch clone, fetch branches, merge, then call
    // run_resolve_in_repo(scratch_repo, args, true) on the scratch repo.
    Err(GcmError::RemoteHost {
        host: remote_ref.host.resolution_slug().to_string(),
        reason: "scratch clone implementation pending in next sub-task".to_string(),
    })
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn dry_run_report(_remote_ref: &RemoteRef) -> ResolveReport {
    ResolveReport {
        v: crate::output::SCHEMA_VERSION,
        status: crate::resolve::report::ResolveStatus::Noop,
        files: vec![],
    }
}
