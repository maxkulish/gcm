use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The grouping plan returned by the provider's structured-output mode
/// (ADR-001 Decision 1). Typed deserialization replaces the bash tool's
/// `sed -> perl -> jq` scrape of reasoning-polluted JSON (FR-16, FR-19).
/// `Serialize`/`Clone` so the plan can be persisted to (and advanced in) the
/// per-repo cache (CLO-491, FR-25).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Plan {
    pub groups: Vec<Group>,
}

/// One logical commit: the files it covers, a one-line summary, and (for
/// `groups[0]` only, per the regenerate-per-group contract) a commit message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Group {
    pub files: Vec<String>,
    pub summary: String,
    /// Full conventional-commit message for `groups[0]`; `null` for later
    /// groups (we re-analyze each run, so their messages are never used here).
    pub commit_message: Option<String>,
}

/// Why a plan was rejected by [`validate_basic`]. Each maps to an announced
/// fallback to the single-commit path (FR-23 basic; full validation is CLO-492).
#[derive(Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The plan has no groups at all (`groups: []`).
    NoGroups,
    /// Group 1 references no files - nothing to commit.
    EmptyFirstGroup,
    /// Group 1 has a null/empty commit message (the exact bash null-message bug).
    MissingFirstMessage,
    /// A plan file is not in the real change set (a hallucinated path).
    UnknownFile(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::NoGroups => write!(f, "plan contained no groups"),
            PlanError::EmptyFirstGroup => write!(f, "group 1 references no files"),
            PlanError::MissingFirstMessage => {
                write!(f, "group 1 has no commit message")
            }
            PlanError::UnknownFile(p) => {
                write!(f, "group 1 references unknown file '{p}'")
            }
        }
    }
}

/// The inner JSON Schema object sent with `response_format` (ADR-001 Decision 5,
/// Groq strict mode): every property is `required` and every object sets
/// `additionalProperties: false`; `commit_message` is nullable so later groups
/// can carry `null`.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "groups": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "files": { "type": "array", "items": { "type": "string" } },
                        "summary": { "type": "string" },
                        "commit_message": { "type": ["string", "null"] }
                    },
                    "required": ["files", "summary", "commit_message"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["groups"],
        "additionalProperties": false
    })
}

/// Basic plan validation (FR-23 basic): the plan must have at least one group,
/// group 1 must be non-empty and carry a usable message, and no group may
/// reference a file absent from the real change set. Full bijective validation
/// (every changed file covered exactly once) is CLO-492.
pub fn validate_basic(plan: &Plan, change_set: &HashSet<String>) -> Result<(), PlanError> {
    let first = plan.groups.first().ok_or(PlanError::NoGroups)?;
    if first.files.is_empty() {
        return Err(PlanError::EmptyFirstGroup);
    }
    match &first.commit_message {
        Some(m) if !m.trim().is_empty() => {}
        _ => return Err(PlanError::MissingFirstMessage),
    }
    // No group may reference a file outside the real change set (catches
    // hallucinated paths). Full coverage/bijection checks are CLO-492.
    for group in &plan.groups {
        for file in &group.files {
            if !change_set.contains(file) {
                return Err(PlanError::UnknownFile(file.clone()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change_set(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    fn parse(json_str: &str) -> Plan {
        serde_json::from_str(json_str).expect("valid plan json")
    }

    #[test]
    fn deserializes_typed_plan() {
        let p = parse(
            r#"{"groups":[
                {"files":["a.rs"],"summary":"core","commit_message":"feat: a"},
                {"files":["b.md"],"summary":"docs","commit_message":null}
            ]}"#,
        );
        assert_eq!(p.groups.len(), 2);
        assert_eq!(p.groups[0].files, vec!["a.rs"]);
        assert_eq!(p.groups[0].commit_message.as_deref(), Some("feat: a"));
        assert_eq!(p.groups[1].commit_message, None);
    }

    #[test]
    fn accepts_a_valid_plan() {
        let p =
            parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"feat: a"}]}"#);
        assert_eq!(validate_basic(&p, &change_set(&["a.rs", "b.md"])), Ok(()));
    }

    #[test]
    fn rejects_empty_groups() {
        let p = parse(r#"{"groups":[]}"#);
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::NoGroups)
        );
    }

    #[test]
    fn rejects_empty_first_group() {
        let p = parse(r#"{"groups":[{"files":[],"summary":"s","commit_message":"feat: a"}]}"#);
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::EmptyFirstGroup)
        );
    }

    #[test]
    fn rejects_null_message_in_group1() {
        // The exact bash null-message bug: must be caught, not silently single-committed.
        let p = parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":null}]}"#);
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::MissingFirstMessage)
        );
    }

    #[test]
    fn rejects_blank_message_in_group1() {
        let p = parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"   "}]}"#);
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::MissingFirstMessage)
        );
    }

    #[test]
    fn rejects_unknown_file() {
        let p = parse(
            r#"{"groups":[
                {"files":["a.rs"],"summary":"s","commit_message":"feat: a"},
                {"files":["ghost.rs"],"summary":"s2","commit_message":null}
            ]}"#,
        );
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::UnknownFile("ghost.rs".to_string()))
        );
    }

    #[test]
    fn schema_is_strict_compatible() {
        let s = schema();
        assert_eq!(s["additionalProperties"], json!(false));
        let item = &s["properties"]["groups"]["items"];
        assert_eq!(item["additionalProperties"], json!(false));
        // strict mode requires every property to be listed in `required`.
        assert_eq!(
            item["required"],
            json!(["files", "summary", "commit_message"])
        );
        assert_eq!(
            item["properties"]["commit_message"]["type"],
            json!(["string", "null"])
        );
    }
}
