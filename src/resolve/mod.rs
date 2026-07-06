//! `gcm resolve` — LLM-assisted merge conflict resolver (Phase 1: local markers).
//!
//! Public entry point is [`run_resolve`] (called from `main.rs` for the
//! `resolve` subcommand). All sub-modules are implementation details.

pub mod classify;
pub mod markers;
pub mod mergiraf;
pub mod prompt;
pub mod report;
pub mod validate;

use std::collections::HashSet;

use crate::cli::{Cli, Commands};
use crate::config::{AutoPolicy, ConflictConfig};
use crate::error::GcmError;
use crate::git::{ChangedFile, Repo};
use crate::output;
use crate::privacy::{Privacy, SecretScanMode};
use crate::provider::{ConflictHunk, Provider, ResolveContext, Resolution};

use classify::{classify, HunkResolution};
use markers::{has_conflict_markers, parse, ConflictFile};
use report::{FileAction, FileReport, ResolveReport, ResolveStatus};
use validate::{validate, ValidationError};

/// Internal result of resolving one file.
#[derive(Debug, Clone)]
struct FileResolution {
    path: String,
    hunks_total: usize,
    hunks_auto: usize,
    hunks_llm: usize,
    hunks_escalated: usize,
    action: FileAction,
}

/// Entry point for `gcm resolve`.
pub fn run_resolve(args: &Cli) -> Result<(), GcmError> {
    let repo = Repo::discover()?.ok_or(GcmError::NotARepo)?;

    if !repo.has_conflict_state() {
        return Err(GcmError::NoConflictInProgress);
    }

    let unmerged = repo.unmerged_files()?;
    if unmerged.is_empty() {
        return Err(GcmError::NoConflicts);
    }

    // Hydrate config so provider/model/env precedence works as usual.
    if let Some(cfg) = crate::config::load() {
        crate::config::apply_to_env(&cfg);
    }

    let conflict = resolve_conflict_config(args);

    // Re-checkout with zdiff3 markers so every file has a parseable base/ours/theirs.
    let paths: Vec<&str> = unmerged.iter().map(String::as_str).collect();
    repo.checkout_conflict_zdiff3(&paths)?;

    let binary = repo.binary_unmerged_files()?;
    let binary_set: HashSet<String> = binary.into_iter().collect();

    let provider = crate::provider::select(args.provider, args.model.as_deref())
        .map_err(GcmError::Provider)?;
    let privacy = Privacy::load(&repo, args.secret_scan)?;

    let mut resolutions = Vec::new();
    let mut any_skipped_or_escalated = false;

    for path in &unmerged {
        let changed = ChangedFile {
            x: b'U',
            y: b'U',
            path: path.clone(),
            orig_path: None,
        };
        if privacy.filter_changed(&[changed]).is_empty() {
            eprintln!("gcm resolve: skipping {path} (excluded by .gcmignore/gcmignore)");
            resolutions.push(FileResolution {
                path: path.clone(),
                hunks_total: 0,
                hunks_auto: 0,
                hunks_llm: 0,
                hunks_escalated: 0,
                action: FileAction::Skipped,
            });
            any_skipped_or_escalated = true;
            continue;
        }

        let file_resolution = resolve_file(
            &repo,
            path,
            &conflict,
            &binary_set,
            provider.as_ref(),
            &privacy,
            args,
        )?;

        match file_resolution.action {
            FileAction::Accepted | FileAction::Edited => {}
            FileAction::Skipped | FileAction::Escalated | FileAction::DryRun => {
                any_skipped_or_escalated = true;
            }
        }
        resolutions.push(file_resolution);
    }

    let status = if resolutions.is_empty() {
        ResolveStatus::Noop
    } else if any_skipped_or_escalated {
        ResolveStatus::Partial
    } else {
        ResolveStatus::Resolved
    };

    let report = ResolveReport {
        v: output::SCHEMA_VERSION,
        status,
        files: resolutions
            .into_iter()
            .map(|r| FileReport {
                path: r.path,
                hunks_total: r.hunks_total,
                hunks_auto: r.hunks_auto,
                hunks_llm: r.hunks_llm,
                hunks_escalated: r.hunks_escalated,
                action: r.action,
            })
            .collect(),
    };

    if args.json {
        report::emit(&report);
    } else {
        print_human_report(&report);
    }

    Ok(())
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
                if c.sensitive_paths.as_ref().map(|v| v.is_empty()).unwrap_or(true) {
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

fn resolve_file(
    repo: &Repo,
    path: &str,
    conflict: &ConflictConfig,
    binary_set: &HashSet<String>,
    provider: &dyn Provider,
    privacy: &Privacy,
    args: &Cli,
) -> Result<FileResolution, GcmError> {
    if binary_set.contains(path) {
        eprintln!("gcm resolve: skipping {path} (binary file)");
        return Ok(FileResolution {
            path: path.to_string(),
            hunks_total: 0,
            hunks_auto: 0,
            hunks_llm: 0,
            hunks_escalated: 0,
            action: FileAction::Escalated,
        });
    }

    if is_sensitive_path(path, &conflict.sensitive_paths) {
        eprintln!("gcm resolve: escalating {path} (matches sensitive_paths)");
        return Ok(FileResolution {
            path: path.to_string(),
            hunks_total: 0,
            hunks_auto: 0,
            hunks_llm: 0,
            hunks_escalated: 0,
            action: FileAction::Escalated,
        });
    }

    let content = repo.read_file(path)?;
    let file = parse(path.to_string(), &content);

    if file.hunks.is_empty() {
        // File was already resolved (e.g. by a prior run) or has no markers.
        return Ok(FileResolution {
            path: path.to_string(),
            hunks_total: 0,
            hunks_auto: 0,
            hunks_llm: 0,
            hunks_escalated: 0,
            action: FileAction::Accepted,
        });
    }

    // Optional mergiraf pre-stage.
    if conflict.mergiraf && mergiraf::try_resolve(repo, path)? {
        let after = repo.read_file(path)?;
        let file = parse(path.to_string(), &after);
        if file.hunks.is_empty() {
            let action = if args.dry_run {
                FileAction::DryRun
            } else {
                // mergiraf already wrote the file; nothing more to do.
                FileAction::Accepted
            };
            return Ok(FileResolution {
                path: path.to_string(),
                hunks_total: 0,
                hunks_auto: 0,
                hunks_llm: 0,
                hunks_escalated: 0,
                action,
            });
        }
    }

    let total = file.hunks.len();
    let mut resolutions: Vec<Option<String>> = vec![None; total];
    let mut auto_count = 0;
    let mut llm_indices = Vec::new();

    for (i, hunk) in file.hunks.iter().enumerate() {
        match classify(hunk) {
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
        if privacy.secret_scan_mode() == SecretScanMode::Abort {
            for i in &llm_indices {
                let h = &file.hunks[*i];
                let combined = format!(
                    "{}{}{}",
                    h.base.as_deref().unwrap_or(""),
                    h.ours,
                    h.theirs
                );
                privacy.scan_text(combined)?;
            }
        }

        let provider_hunks: Vec<ConflictHunk> = llm_indices
            .iter()
            .map(|i| ConflictHunk {
                base: file.hunks[*i].base.clone(),
                ours: file.hunks[*i].ours.clone(),
                theirs: file.hunks[*i].theirs.clone(),
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
        for batch in batches {
            let mut batch_results = provider.resolve_hunks(&batch)?;
            llm_results.append(&mut batch_results);
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

    // Validation gate.
    let validated_text = match validate(
        &resolved_text,
        conflict.validate_cmd.as_deref(),
        repo,
        path,
    ) {
        Ok(()) => resolved_text,
        Err(ValidationError::ConflictMarkers) => {
            // One bounded retry: ask the provider to fix its own output.
            attempt_validation_retry(provider, &file, &resolutions, conflict.temperature, repo, path)?
        }
        Err(ValidationError::ValidateCmdFailed { .. }) => {
            escalated_count += llm_count;
            llm_count = 0;
            return Ok(FileResolution {
                path: path.to_string(),
                hunks_total: total,
                hunks_auto: auto_count,
                hunks_llm: 0,
                hunks_escalated: escalated_count,
                action: FileAction::Escalated,
            });
        }
    };

    if args.dry_run {
        if !args.json {
            eprintln!(
                "gcm resolve: {path} would be resolved ({auto_count} auto, {llm_count} LLM)"
            );
        }
        return Ok(FileResolution {
            path: path.to_string(),
            hunks_total: total,
            hunks_auto: auto_count,
            hunks_llm: llm_count,
            hunks_escalated: escalated_count,
            action: FileAction::DryRun,
        });
    }

    // Per-file preview loop.
    let action = if args.yes {
        if escalated_count > 0 {
            FileAction::Escalated
        } else {
            repo.write_file(path, &validated_text)?;
            FileAction::Accepted
        }
    } else {
        match crate::ui::confirm_file(path, &validated_text, args.json)? {
            crate::ui::FileDecision::Accept => {
                repo.write_file(path, &validated_text)?;
                FileAction::Accepted
            }
            crate::ui::FileDecision::Skip => FileAction::Skipped,
            crate::ui::FileDecision::Edit => {
                let edited = crate::ui::edit_in_editor(&validated_text)?;
                validate(
                    &edited,
                    conflict.validate_cmd.as_deref(),
                    repo,
                    path,
                )
                .map_err(|e| GcmError::ResolutionEscalated {
                    path: path.to_string(),
                    reason: format!("edited content failed validation: {e:?}"),
                })?;
                repo.write_file(path, &edited)?;
                FileAction::Edited
            }
        }
    };

    Ok(FileResolution {
        path: path.to_string(),
        hunks_total: total,
        hunks_auto: auto_count,
        hunks_llm: llm_count,
        hunks_escalated: escalated_count,
        action,
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
    let text = reconstruct(file, &new_resolutions, "");
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
    let mut out = String::new();
    let mut hunk_idx = 0;
    let mut line_no = 1usize;
    while line_no <= original_lines.len() {
        if hunk_idx < file.hunks.len() && line_no == file.hunks[hunk_idx].start_line {
            if let Some(text) = &resolutions[hunk_idx] {
                out.push_str(text);
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
    match &report.status {
        ResolveStatus::Resolved => println!("All conflicts resolved."),
        ResolveStatus::Partial => {
            println!("Some files resolved; others were skipped or escalated.");
        }
        ResolveStatus::Noop => println!("No conflicts to resolve."),
        ResolveStatus::Error => println!("Resolution failed."),
    }
    for f in &report.files {
        println!(
            "  {}: {} total, {} auto, {} LLM, {} escalated ({:?})",
            f.path, f.hunks_total, f.hunks_auto, f.hunks_llm, f.hunks_escalated, f.action
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_basic() {
        assert!(glob_match("*.rs", "src/lib.rs"));
        assert!(glob_match("secrets/*", "secrets/key.pem"));
        assert!(!glob_match("secrets/*", "src/secrets/key.pem"));
        assert!(glob_match("?.*", "a.rs"));
    }

    #[test]
    fn is_sensitive_path_matches() {
        assert!(is_sensitive_path("secrets/key.pem", &["secrets/*".to_string()]));
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
