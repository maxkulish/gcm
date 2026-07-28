//! JSON report envelope for `gcm resolve --json` (CLO-531, ST4).
//!
//! This is intentionally a separate envelope from the commit-flow `Envelope`:
//! resolve reports per-file hunk breakdowns and actions rather than commit
//! summaries.

use serde::Serialize;

use crate::resolve::remote::host::Host;

/// The `--json` envelope for `gcm resolve`. The CLO-555 fields (`staged`,
/// `finish`, `restored`) are additive and omitted when empty/absent/false, so
/// a run that touches none of them emits byte-identical JSON to before.
#[derive(Debug, Serialize)]
pub struct ResolveReport {
    pub v: i32,
    pub status: ResolveStatus,
    pub files: Vec<FileReport>,
    /// Paths staged in the apply phase.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub staged: Vec<String>,
    /// Outcome of the finishing step (merge commit / rebase / cherry-pick).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish: Option<FinishReport>,
    /// True when a user rejection restored the pre-run working tree.
    #[serde(skip_serializing_if = "is_false")]
    pub restored: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteReport>,
    /// Per-round detail for a rebase/cherry-pick sequence driven to completion
    /// (CLO-554). Present only when the loop actually engaged - more than one
    /// round ran, or the run stopped at the loop's own boundary. Every scenario
    /// that predates CLO-554 therefore emits the envelope unchanged.
    #[serde(rename = "loop", skip_serializing_if = "Option::is_none")]
    pub loop_report: Option<LoopReport>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Summary of a multi-round resolve. `rounds` carries one entry per round
/// attempted, in order; the top-level `files`/`staged`/`finish` always describe
/// the last of them.
#[derive(Debug, Serialize)]
pub struct LoopReport {
    pub rounds_run: usize,
    /// The cap in force for this run (CLI flag, else config, else 10).
    pub max_rounds: u32,
    pub terminal: LoopTerminal,
    /// Short sha the operation is parked on when the loop ends without
    /// finishing the sequence, so a machine consumer can name the stop without
    /// shelling out to git. Absent once the sequence completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped_on: Option<String>,
    pub rounds: Vec<RoundReport>,
}

/// One conflict stop resolved (or attempted) inside the loop.
#[derive(Debug, Serialize)]
pub struct RoundReport {
    /// 1-based round number.
    pub round: usize,
    /// Short sha of the commit this round was applying, read from
    /// `REBASE_HEAD`/`CHERRY_PICK_HEAD`. Absent for a merge, which has no
    /// per-step head.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub status: ResolveStatus,
    pub files: Vec<FileReport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub staged: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish: Option<FinishReport>,
}

/// Why the loop stopped. Every variant is an exit-0 outcome; a finish that
/// genuinely failed still surfaces as the `FinishFailed` error envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopTerminal {
    /// The sequence finished: nothing left to continue.
    Completed,
    /// The round cap was reached with the operation still stopped.
    CapReached,
    /// The user declined the round gate; nothing was spent on the next round.
    Declined,
    /// The user rejected a file inside a round: that round was restored, the
    /// earlier rounds stay committed.
    Aborted,
    /// A round escalated, so its finish was skipped and no next stop exists.
    Partial,
    /// The operation head did not advance between rounds - a stall the driver
    /// refuses to spend another round on.
    NoProgress,
}

/// Outcome of the finishing step, mirroring `git::FinishOutcome` in stable
/// snake_case for machine consumers.
#[derive(Debug, Clone, Serialize)]
pub struct FinishReport {
    pub result: FinishResult,
    /// Short sha of the finishing commit (present only on `completed`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The operation that was (or would be) finished: merge / rebase /
    /// cherry-pick. Absent when no operation ref exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishResult {
    /// The operation was completed by a signed commit / continue.
    Completed,
    /// A rebase/cherry-pick continued and stopped on its next conflicted
    /// commit (re-run `gcm resolve`).
    StoppedOnConflict,
    /// The finish was not attempted (`--no-finish`, escalations present, or
    /// no operation ref to finish). A finish that ran and FAILED is not a
    /// report value: it surfaces as the `FinishFailed` error envelope with a
    /// non-zero exit, staged state kept.
    Skipped,
}

#[derive(Debug, Serialize)]
pub struct RemoteReport {
    pub host: Host,
    pub number: u64,
    pub base_branch: String,
    pub source_branch: String,
    pub resolution_branch: String,
    pub pushed: bool,
    pub commented: bool,
    /// Path to the scratch repo (preserved on success, per AC7).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scratch_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveStatus {
    /// All non-escalated files were accepted.
    Resolved,
    /// Some files accepted, some skipped/escalated.
    Partial,
    /// No conflicts found or all files already resolved.
    Noop,
    /// The user rejected a proposal: the pre-run working tree was restored
    /// and nothing was applied (exit 0).
    Aborted,
    /// A fatal error aborted the run.
    #[allow(dead_code)]
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileReport {
    pub path: String,
    pub hunks_total: usize,
    pub hunks_auto: usize,
    pub hunks_llm: usize,
    pub hunks_escalated: usize,
    pub action: FileAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Accepted,
    Skipped,
    Edited,
    Escalated,
    DryRun,
    /// The user answered No to this file's proposal, aborting the run.
    Rejected,
}

impl ResolveReport {
    /// Human-readable status label for non-JSON output.
    pub fn status_label(&self) -> &'static str {
        match self.status {
            ResolveStatus::Resolved => "resolved",
            ResolveStatus::Partial => "partial",
            ResolveStatus::Noop => "noop",
            ResolveStatus::Aborted => "aborted",
            ResolveStatus::Error => "error",
        }
    }
}

/// Serialize and emit the report to stdout. This is the only place `gcm resolve`
/// writes JSON to stdout.
pub fn emit(report: &ResolveReport) {
    println!(
        "{}",
        serde_json::to_string(report)
            .unwrap_or_else(|_| { "{\"v\":1,\"status\":\"error\",\"files\":[]}".to_string() })
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_report_serializes_to_expected_shape() {
        let report = ResolveReport {
            v: 1,
            status: ResolveStatus::Partial,
            files: vec![FileReport {
                path: "src/lib.rs".to_string(),
                hunks_total: 3,
                hunks_auto: 1,
                hunks_llm: 1,
                hunks_escalated: 1,
                action: FileAction::Accepted,
            }],
            staged: vec![],
            finish: None,
            restored: false,
            remote: None,
            loop_report: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"status\":\"partial\""));
        assert!(json.contains("\"hunks_total\":3"));
        assert!(json.contains("\"action\":\"accepted\""));
        // Empty/false/absent CLO-555 fields are omitted entirely.
        assert!(!json.contains("staged"));
        assert!(!json.contains("finish"));
        assert!(!json.contains("restored"));
        // ...and so is the CLO-554 loop block when the loop never engaged.
        assert!(!json.contains("loop"));
    }

    #[test]
    fn resolve_report_new_fields_serialize_when_set() {
        let report = ResolveReport {
            v: 1,
            status: ResolveStatus::Aborted,
            files: vec![],
            staged: vec!["a.txt".to_string()],
            finish: Some(FinishReport {
                result: FinishResult::StoppedOnConflict,
                commit: None,
                op: Some("rebase".to_string()),
            }),
            restored: true,
            remote: None,
            loop_report: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"status\":\"aborted\""));
        assert!(json.contains("\"staged\":[\"a.txt\"]"));
        assert!(json.contains("\"result\":\"stopped_on_conflict\""));
        assert!(json.contains("\"restored\":true"));
        assert!(
            !json.contains("commit"),
            "absent commit sha is omitted: {json}"
        );
    }

    #[test]
    fn loop_report_serializes_rounds_and_terminal() {
        let report = ResolveReport {
            v: 1,
            status: ResolveStatus::Resolved,
            files: vec![],
            staged: vec![],
            finish: None,
            restored: false,
            remote: None,
            loop_report: Some(LoopReport {
                rounds_run: 2,
                max_rounds: 10,
                terminal: LoopTerminal::Completed,
                stopped_on: None,
                rounds: vec![
                    RoundReport {
                        round: 1,
                        commit: Some("a1b2c3d".to_string()),
                        status: ResolveStatus::Resolved,
                        files: vec![FileReport {
                            path: "f.txt".to_string(),
                            hunks_total: 1,
                            hunks_auto: 0,
                            hunks_llm: 1,
                            hunks_escalated: 0,
                            action: FileAction::Accepted,
                        }],
                        staged: vec!["f.txt".to_string()],
                        finish: Some(FinishReport {
                            result: FinishResult::StoppedOnConflict,
                            commit: None,
                            op: Some("rebase".to_string()),
                        }),
                    },
                    RoundReport {
                        round: 2,
                        commit: Some("b2c3d4e".to_string()),
                        status: ResolveStatus::Resolved,
                        files: vec![],
                        staged: vec![],
                        finish: Some(FinishReport {
                            result: FinishResult::Completed,
                            commit: Some("9f8e7d6".to_string()),
                            op: Some("rebase".to_string()),
                        }),
                    },
                ],
            }),
        };
        let json = serde_json::to_string(&report).unwrap();
        // The Rust field is `loop_report`; the wire name is the bare `loop`.
        assert!(json.contains("\"loop\":{"), "{json}");
        assert!(!json.contains("loop_report"), "{json}");
        assert!(json.contains("\"rounds_run\":2"));
        assert!(json.contains("\"max_rounds\":10"));
        assert!(json.contains("\"terminal\":\"completed\""));
        assert!(json.contains("\"round\":1"));
        assert!(json.contains("\"commit\":\"a1b2c3d\""));
        // An empty round (all files already resolved) omits its empty vectors.
        assert!(json.contains("\"round\":2"));
    }

    #[test]
    fn loop_terminal_snake_cases() {
        assert_eq!(
            serde_json::to_string(&LoopTerminal::CapReached).unwrap(),
            "\"cap_reached\""
        );
        assert_eq!(
            serde_json::to_string(&LoopTerminal::NoProgress).unwrap(),
            "\"no_progress\""
        );
        assert_eq!(
            serde_json::to_string(&LoopTerminal::Declined).unwrap(),
            "\"declined\""
        );
    }

    #[test]
    fn file_action_snake_cases() {
        assert_eq!(
            serde_json::to_string(&FileAction::DryRun).unwrap(),
            "\"dry_run\""
        );
    }
}
