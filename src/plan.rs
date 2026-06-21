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

/// Why a plan was rejected by [`validate`]. Each maps to an announced fallback
/// to the single-commit path (FR-23 full: the plan must partition the change set
/// - no omissions, no duplicates, no empty groups, message on `groups[0]`).
#[derive(Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The plan has no groups at all (`groups: []`).
    NoGroups,
    /// A group references no files (0-based group index; rendered 1-based).
    EmptyGroup(usize),
    /// Group 1 has a null/empty commit message (the exact bash null-message bug).
    MissingFirstMessage,
    /// A plan file is not in the real change set (a hallucinated path).
    UnknownFile(String),
    /// A changed file is listed in more than one group (or twice in one group);
    /// staging would record it under two commits (FR-23 case c).
    DuplicateFile(String),
    /// A changed file appears in no group at all - the bash validator missed
    /// this and silently dropped the file from history (FR-23 case b).
    OmittedFile(String),
    /// The response could not be parsed into a plan at all (FR-20 defensive
    /// parsing exhausted every candidate).
    Parse(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::NoGroups => write!(f, "plan contained no groups"),
            PlanError::EmptyGroup(i) => write!(f, "group {} references no files", i + 1),
            PlanError::MissingFirstMessage => {
                write!(f, "group 1 has no commit message")
            }
            PlanError::UnknownFile(p) => {
                write!(f, "plan references unknown file '{p}'")
            }
            PlanError::DuplicateFile(p) => {
                write!(f, "file '{p}' appears in more than one group")
            }
            PlanError::OmittedFile(p) => {
                write!(f, "plan omitted changed file '{p}'")
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
    // A bare top-level array is treated as the groups array itself (a model that
    // dropped the {"groups": ..} wrapper). The re-wrap + Plan deserialize still
    // validates the element shape, so a non-group array simply fails downstream.
    if v.is_array() {
        return Some(v.clone());
    }
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

/// Full plan validation (FR-23): the plan must **partition** the change set.
/// Rejected (each -> an announced single-commit fallback) when any of these hold,
/// checked in this deterministic order so the reported error is stable:
///
/// 1. no groups at all (`NoGroups`);
/// 2. any group references no files (`EmptyGroup`, first offending group);
/// 3. `groups[0]` has a null/blank commit message (`MissingFirstMessage` - the
///    FR-45 placement check; later groups' messages are tolerated/ignored);
/// 4. any group references a path absent from the change set (`UnknownFile`,
///    first offending file in group-then-file order);
/// 5. any path appears in more than one group, or twice in one group
///    (`DuplicateFile`, first repeat);
/// 6. any changed file is covered by no group (`OmittedFile`, first omission in
///    sorted order for determinism over the unordered change set).
///
/// (4)+(5)+(6) together make the union of group files a bijection with the
/// change set. The validator is pure: no git calls, `change_set` is supplied by
/// the caller (`changed.iter().map(|c| c.path)`).
pub fn validate(plan: &Plan, change_set: &HashSet<String>) -> Result<(), PlanError> {
    check_structure(plan)?;
    // FR-45 message placement: a freshly generated plan must carry the group-0
    // message. (Not checked on the cache-hit path - see `validate_cached`.)
    match &plan.groups[0].commit_message {
        Some(m) if !m.trim().is_empty() => {}
        _ => return Err(PlanError::MissingFirstMessage),
    }
    validate_partition(plan, change_set)
}

/// Partition-only re-validation for a **cached** plan, safe to run on the
/// cache-hit path: it enforces the same bijection (no empty groups, no unknowns,
/// no duplicates, no omissions) but **skips the `groups[0]` message check**. An
/// advanced cache entry legitimately carries a null first message (regenerated
/// per group, ADR-001 Decision 6), so checking it would wrongly reject the normal
/// "commit the next group" flow. This is defense in depth: a plan written by a
/// pre-CLO-492 binary (which only screened unknown files) - or any future
/// advance defect - must still partition the current change set, or grouping
/// would silently drop/duplicate a file (the FR-23 bug this slice fixes).
pub fn validate_cached(plan: &Plan, change_set: &HashSet<String>) -> Result<(), PlanError> {
    check_structure(plan)?;
    validate_partition(plan, change_set)
}

/// Structural checks shared by [`validate`] and [`validate_cached`]: at least one
/// group, and no empty group (first offender, 0-based).
fn check_structure(plan: &Plan) -> Result<(), PlanError> {
    if plan.groups.is_empty() {
        return Err(PlanError::NoGroups);
    }
    for (i, group) in plan.groups.iter().enumerate() {
        if group.files.is_empty() {
            return Err(PlanError::EmptyGroup(i));
        }
    }
    Ok(())
}

/// The bijection check shared by [`validate`] and [`validate_cached`]: every plan
/// path is known (in the change set) and unique (no file in two groups, or twice
/// in one), and every changed file is covered by some group.
fn validate_partition(plan: &Plan, change_set: &HashSet<String>) -> Result<(), PlanError> {
    // Single walk over every plan path: reject the first unknown (outside the
    // change set) or duplicate (already seen in any group). `seen` doubles as the
    // coverage set for the omission check below.
    let mut seen: HashSet<&str> = HashSet::with_capacity(change_set.len());
    for group in &plan.groups {
        for file in &group.files {
            if !change_set.contains(file) {
                return Err(PlanError::UnknownFile(file.clone()));
            }
            if !seen.insert(file.as_str()) {
                return Err(PlanError::DuplicateFile(file.clone()));
            }
        }
    }
    // Every changed file must be covered. Sort the omissions so the reported file
    // is deterministic over the unordered `change_set`.
    let mut omitted: Vec<&String> = change_set
        .iter()
        .filter(|f| !seen.contains(f.as_str()))
        .collect();
    if !omitted.is_empty() {
        omitted.sort();
        return Err(PlanError::OmittedFile(omitted[0].clone()));
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
    fn accepts_a_valid_partition() {
        // groups[0] covers a.rs (with message), group 2 covers b.md - exact partition.
        let p = parse(
            r#"{"groups":[
                {"files":["a.rs"],"summary":"core","commit_message":"feat: a"},
                {"files":["b.md"],"summary":"docs","commit_message":null}
            ]}"#,
        );
        assert_eq!(validate(&p, &change_set(&["a.rs", "b.md"])), Ok(()));
    }

    #[test]
    fn accepts_single_group_covering_all() {
        let p = parse(
            r#"{"groups":[{"files":["a.rs","b.md"],"summary":"s","commit_message":"feat: a"}]}"#,
        );
        assert_eq!(validate(&p, &change_set(&["a.rs", "b.md"])), Ok(()));
    }

    #[test]
    fn rejects_no_groups() {
        let p = parse(r#"{"groups":[]}"#);
        assert_eq!(
            validate(&p, &change_set(&["a.rs"])),
            Err(PlanError::NoGroups)
        );
    }

    #[test]
    fn rejects_empty_first_group() {
        let p = parse(r#"{"groups":[{"files":[],"summary":"s","commit_message":"feat: a"}]}"#);
        assert_eq!(
            validate(&p, &change_set(&["a.rs"])),
            Err(PlanError::EmptyGroup(0))
        );
    }

    #[test]
    fn rejects_empty_group_at_later_index() {
        // FR-23 case d: an empty group anywhere, not just group 1.
        let p = parse(
            r#"{"groups":[
                {"files":["a.rs"],"summary":"s","commit_message":"feat: a"},
                {"files":[],"summary":"empty","commit_message":null}
            ]}"#,
        );
        assert_eq!(
            validate(&p, &change_set(&["a.rs"])),
            Err(PlanError::EmptyGroup(1))
        );
    }

    #[test]
    fn rejects_null_message_in_group1() {
        // The exact bash null-message bug: must be caught, not silently single-committed.
        let p = parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":null}]}"#);
        assert_eq!(
            validate(&p, &change_set(&["a.rs"])),
            Err(PlanError::MissingFirstMessage)
        );
    }

    #[test]
    fn rejects_blank_message_in_group1() {
        let p = parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"   "}]}"#);
        assert_eq!(
            validate(&p, &change_set(&["a.rs"])),
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
            validate(&p, &change_set(&["a.rs"])),
            Err(PlanError::UnknownFile("ghost.rs".to_string()))
        );
    }

    #[test]
    fn rejects_omitted_file() {
        // FR-23 case b: b.md changed but covered by no group -> reject, do not drop.
        let p =
            parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"feat: a"}]}"#);
        assert_eq!(
            validate(&p, &change_set(&["a.rs", "b.md"])),
            Err(PlanError::OmittedFile("b.md".to_string()))
        );
    }

    #[test]
    fn rejects_cross_group_duplicate() {
        // FR-23 case c: a.rs listed in two groups.
        let p = parse(
            r#"{"groups":[
                {"files":["a.rs"],"summary":"s","commit_message":"feat: a"},
                {"files":["a.rs"],"summary":"s2","commit_message":null}
            ]}"#,
        );
        assert_eq!(
            validate(&p, &change_set(&["a.rs"])),
            Err(PlanError::DuplicateFile("a.rs".to_string()))
        );
    }

    #[test]
    fn rejects_same_group_duplicate() {
        // Degenerate duplicate: the same file twice within one group.
        let p = parse(
            r#"{"groups":[{"files":["a.rs","a.rs"],"summary":"s","commit_message":"feat: a"}]}"#,
        );
        assert_eq!(
            validate(&p, &change_set(&["a.rs"])),
            Err(PlanError::DuplicateFile("a.rs".to_string()))
        );
    }

    #[test]
    fn covers_rename_by_new_path() {
        // The validator uses the (new) path only; a rename's new path covers it.
        let p = parse(
            r#"{"groups":[{"files":["new.rs"],"summary":"s","commit_message":"feat: rename"}]}"#,
        );
        assert_eq!(validate(&p, &change_set(&["new.rs"])), Ok(()));
    }

    #[test]
    fn plan_error_display_is_distinct() {
        let msgs = [
            PlanError::NoGroups.to_string(),
            PlanError::EmptyGroup(1).to_string(),
            PlanError::MissingFirstMessage.to_string(),
            PlanError::UnknownFile("x".into()).to_string(),
            PlanError::DuplicateFile("x".into()).to_string(),
            PlanError::OmittedFile("x".into()).to_string(),
            PlanError::Parse("x".into()).to_string(),
        ];
        assert!(msgs.iter().all(|m| !m.is_empty()), "no empty Display");
        let unique: HashSet<&String> = msgs.iter().collect();
        assert_eq!(unique.len(), msgs.len(), "all 7 Display strings distinct");
        // 0-based store, 1-based render.
        assert!(PlanError::EmptyGroup(1).to_string().contains("group 2"));
    }

    #[test]
    fn validate_cached_tolerates_null_first_message() {
        // An advanced cache entry has a null groups[0] message (regenerated per
        // group). The full `validate` rejects that (MissingFirstMessage), but
        // `validate_cached` must accept it as long as the partition holds.
        let p = parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":null}]}"#);
        assert_eq!(
            validate(&p, &change_set(&["a.rs"])),
            Err(PlanError::MissingFirstMessage),
            "full validate still requires the group-0 message"
        );
        assert_eq!(
            validate_cached(&p, &change_set(&["a.rs"])),
            Ok(()),
            "validate_cached tolerates the null first message"
        );
    }

    #[test]
    fn validate_cached_still_enforces_the_partition() {
        // Omission, duplicate, empty group, and unknown file are all rejected on
        // the cache-hit path too - the bijection is enforced regardless of source.
        let omit = parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":null}]}"#);
        assert_eq!(
            validate_cached(&omit, &change_set(&["a.rs", "b.md"])),
            Err(PlanError::OmittedFile("b.md".to_string()))
        );
        let dup = parse(
            r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":null},{"files":["a.rs"],"summary":"s2","commit_message":null}]}"#,
        );
        assert_eq!(
            validate_cached(&dup, &change_set(&["a.rs"])),
            Err(PlanError::DuplicateFile("a.rs".to_string()))
        );
        let empty = parse(
            r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":null},{"files":[],"summary":"e","commit_message":null}]}"#,
        );
        assert_eq!(
            validate_cached(&empty, &change_set(&["a.rs"])),
            Err(PlanError::EmptyGroup(1))
        );
        let ghost =
            parse(r#"{"groups":[{"files":["ghost.rs"],"summary":"s","commit_message":null}]}"#);
        assert_eq!(
            validate_cached(&ghost, &change_set(&["a.rs"])),
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
    fn parse_defensive_bare_top_level_array() {
        // Codex validation HIGH: a model may emit the plan as a bare array of groups
        // (no {"groups": ..} wrapper); recover it by treating the array AS the groups.
        let s = r#"[{"files":["a"],"summary":"s","commit_message":"feat: a"},{"files":["b"],"summary":"s2","commit_message":null}]"#;
        let p = parse_defensive(s).unwrap();
        assert_eq!(p.groups.len(), 2);
        assert_eq!(p.groups[0].files, vec!["a"]);
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
