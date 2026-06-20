use std::collections::HashSet;
use std::fmt;

use serde::Deserialize;
use serde_json::{json, Value};

/// The grouping plan returned by the provider's structured-output mode
/// (ADR-001 Decision 1). Typed deserialization replaces the bash tool's
/// `sed -> perl -> jq` scrape of reasoning-polluted JSON (FR-16, FR-19).
#[derive(Debug, Deserialize)]
pub struct Plan {
    pub groups: Vec<Group>,
}

/// One logical commit: the files it covers, a one-line summary, and (for
/// `groups[0]` only, per the regenerate-per-group contract) a commit message.
#[derive(Debug, Deserialize)]
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
    /// The response could not be parsed into a plan at all (FR-20 defensive
    /// parsing exhausted every candidate).
    Parse(String),
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
            PlanError::Parse(msg) => write!(f, "plan parse error: {msg}"),
        }
    }
}

/// Recover a typed [`Plan`] from possibly-noisy model output (FR-20). Tries each
/// candidate JSON string (per-fence inner content -> every balanced `{...}`
/// substring -> the whole content) and, per candidate, either a direct
/// `Plan` parse or a `groups`-array recovery re-wrapped as `{"groups": ..}`;
/// returns the first that yields a `Plan`. The `<think>` strip happens upstream,
/// so candidates are already reasoning-free.
pub fn parse_defensive(content: &str) -> Result<Plan, PlanError> {
    for cand in candidates(content) {
        if let Ok(plan) = serde_json::from_str::<Plan>(&cand) {
            return Ok(plan);
        }
        if let Ok(value) = serde_json::from_str::<Value>(&cand) {
            if let Some(groups) = recover_groups(&value) {
                if let Ok(plan) = serde_json::from_value::<Plan>(json!({ "groups": groups })) {
                    return Ok(plan);
                }
            }
        }
    }
    Err(PlanError::Parse(
        "could not extract a commit plan from the response".to_string(),
    ))
}

/// Ordered JSON candidates to try: each fenced block's inner content (highest
/// signal), then every balanced `{...}` substring, then the whole content.
fn candidates(content: &str) -> Vec<String> {
    let mut out = fenced_blocks(content);
    out.extend(balanced_objects(content));
    let whole = content.trim();
    if !whole.is_empty() {
        out.push(whole.to_string());
    }
    out
}

/// Inner content of each ```` ``` ````-fenced block (optional language tag on the
/// opening line), in document order. Each block is a separate candidate - blocks
/// are never concatenated (review point 3). Unterminated fences are ignored.
fn fenced_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut rest = content;
    while let Some(open) = rest.find("```") {
        let after_open = &rest[open + 3..];
        let Some(nl) = after_open.find('\n') else {
            break; // opening fence with no newline -> nothing closeable
        };
        let body = &after_open[nl + 1..];
        let Some(close) = body.find("```") else {
            break; // unterminated fence
        };
        blocks.push(body[..close].trim().to_string());
        rest = &body[close + 3..];
    }
    blocks
}

/// Every balanced `{...}` object substring (string- and escape-aware), in
/// document order. A decoy `{...}` in prose becomes its own candidate and is
/// simply rejected by the parse, so the real block downstream is still reached
/// (review point 1).
fn balanced_objects(content: &str) -> Vec<String> {
    let bytes = content.as_bytes();
    let mut objs = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = balanced_end(bytes, i) {
                objs.push(content[i..=end].to_string());
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    objs
}

/// Index of the `}` closing the `{` at `start`, honoring JSON string literals and
/// `\` escapes so a brace inside a string value cannot close the object; `None`
/// if unbalanced. Braces/quotes/backslash are ASCII, so byte indexing stays on
/// UTF-8 boundaries.
fn balanced_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &c) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Recover the `groups` array from a parsed value, in precedence order: top-level
/// `groups` -> a `groups` array under a known wrapper key (in order) -> a
/// depth-first search for the first `groups` array anywhere.
fn recover_groups(v: &Value) -> Option<Value> {
    if let Some(arr) = v.get("groups").filter(|g| g.is_array()) {
        return Some(arr.clone());
    }
    for key in ["commit_plan", "plan", "result", "data", "response"] {
        if let Some(arr) = v
            .get(key)
            .and_then(|inner| inner.get("groups"))
            .filter(|g| g.is_array())
        {
            return Some(arr.clone());
        }
    }
    find_groups_dfs(v)
}

/// Depth-first search for the first object key `groups` holding an array.
fn find_groups_dfs(v: &Value) -> Option<Value> {
    match v {
        Value::Object(map) => {
            if let Some(arr) = map.get("groups").filter(|g| g.is_array()) {
                return Some(arr.clone());
            }
            map.values().find_map(find_groups_dfs)
        }
        Value::Array(items) => items.iter().find_map(find_groups_dfs),
        _ => None,
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
    fn parse_defensive_direct() {
        let p = parse_defensive(
            r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"feat: a"}]}"#,
        )
        .unwrap();
        assert_eq!(p.groups.len(), 1);
        assert_eq!(p.groups[0].files, vec!["a.rs"]);
    }

    #[test]
    fn parse_defensive_fenced_json() {
        let s = "```json\n{\"groups\":[{\"files\":[\"a\"],\"summary\":\"s\",\"commit_message\":\"feat: a\"}]}\n```";
        assert_eq!(parse_defensive(s).unwrap().groups.len(), 1);
    }

    #[test]
    fn parse_defensive_plain_fence_no_lang_tag() {
        let s = "```\n{\"groups\":[{\"files\":[\"a\"],\"summary\":\"s\",\"commit_message\":\"feat: a\"}]}\n```";
        assert_eq!(parse_defensive(s).unwrap().groups.len(), 1);
    }

    #[test]
    fn parse_defensive_prose_with_decoy_brace() {
        // Review point 1: a decoy {..} precedes the real JSON; must skip it.
        let s = r#"Here is my plan {it's solid}: {"groups":[{"files":["a"],"summary":"s","commit_message":"feat: a"}]}"#;
        assert_eq!(parse_defensive(s).unwrap().groups[0].files, vec!["a"]);
    }

    #[test]
    fn parse_defensive_multi_fence_first_block_is_plan() {
        // Review point 3: two fenced blocks; only the first is a plan; never concatenated.
        let s = "intro\n```json\n{\"groups\":[{\"files\":[\"a\"],\"summary\":\"s\",\"commit_message\":\"feat: a\"}]}\n```\nmiddle\n```json\n{\"other\":true}\n```";
        assert_eq!(parse_defensive(s).unwrap().groups.len(), 1);
    }

    #[test]
    fn parse_defensive_wrapper_key() {
        let s = r#"{"commit_plan":{"groups":[{"files":["a"],"summary":"s","commit_message":"feat: a"}]}}"#;
        assert_eq!(parse_defensive(s).unwrap().groups[0].files, vec!["a"]);
    }

    #[test]
    fn parse_defensive_nested_key_via_dfs() {
        let s = r#"{"result":{"plan":{"groups":[{"files":["a"],"summary":"s","commit_message":"feat: a"}]}}}"#;
        assert_eq!(parse_defensive(s).unwrap().groups.len(), 1);
    }

    #[test]
    fn parse_defensive_recovers_and_rewraps_groups_array() {
        // Review point 2: the recovered `groups` is a bare array, re-wrapped before from_value.
        let s = r#"garbage {"data":{"groups":[{"files":["a"],"summary":"s","commit_message":"feat: a"}]}} trailer"#;
        assert_eq!(parse_defensive(s).unwrap().groups.len(), 1);
    }

    #[test]
    fn parse_defensive_brace_inside_string_value() {
        // Hardening: a brace inside a JSON string must not truncate the candidate.
        let s = r#"{"groups":[{"files":["a}b.txt"],"summary":"s","commit_message":"feat: a"}]}"#;
        assert_eq!(parse_defensive(s).unwrap().groups[0].files, vec!["a}b.txt"]);
    }

    #[test]
    fn parse_defensive_garbage_is_parse_error() {
        match parse_defensive("not json at all") {
            Err(PlanError::Parse(_)) => {}
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_display() {
        assert_eq!(
            PlanError::Parse("boom".to_string()).to_string(),
            "plan parse error: boom"
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
