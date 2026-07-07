//! JSON report envelope for `gcm resolve --json` (CLO-531, ST4).
//!
//! This is intentionally a separate envelope from the commit-flow `Envelope`:
//! resolve reports per-file hunk breakdowns and actions rather than commit
//! summaries.

use serde::Serialize;

/// The `--json` envelope for `gcm resolve`.
#[derive(Debug, Serialize)]
pub struct ResolveReport {
    pub v: i32,
    pub status: ResolveStatus,
    pub files: Vec<FileReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveStatus {
    /// All non-escalated files were accepted.
    Resolved,
    /// Some files accepted, some skipped/escalated.
    Partial,
    /// No conflicts found or all files already resolved.
    Noop,
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
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"status\":\"partial\""));
        assert!(json.contains("\"hunks_total\":3"));
        assert!(json.contains("\"action\":\"accepted\""));
    }

    #[test]
    fn file_action_snake_cases() {
        assert_eq!(
            serde_json::to_string(&FileAction::DryRun).unwrap(),
            "\"dry_run\""
        );
    }
}
