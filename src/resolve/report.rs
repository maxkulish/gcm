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
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Outcome of the finishing step, mirroring `git::FinishOutcome` in stable
/// snake_case for machine consumers.
#[derive(Debug, Serialize)]
pub struct FinishReport {
    pub result: FinishResult,
    /// Short sha of the finishing commit (present only on `completed`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum FinishResult {
    /// The operation was completed by a signed commit / continue.
    Completed,
    /// A rebase/cherry-pick continued and stopped on its next conflicted
    /// commit (re-run `gcm resolve`).
    StoppedOnConflict,
    /// The finishing command failed; staged state is kept.
    Failed,
    /// The finish was not attempted (`--no-finish`, escalations present, or
    /// no operation ref to finish).
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

#[derive(Debug, PartialEq, Serialize)]
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

#[derive(Debug, Serialize)]
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
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"status\":\"partial\""));
        assert!(json.contains("\"hunks_total\":3"));
        assert!(json.contains("\"action\":\"accepted\""));
        // Empty/false/absent CLO-555 fields are omitted entirely.
        assert!(!json.contains("staged"));
        assert!(!json.contains("finish"));
        assert!(!json.contains("restored"));
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
            }),
            restored: true,
            remote: None,
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
    fn file_action_snake_cases() {
        assert_eq!(
            serde_json::to_string(&FileAction::DryRun).unwrap(),
            "\"dry_run\""
        );
    }
}
