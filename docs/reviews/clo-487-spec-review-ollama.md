# Spec Review: clo-487

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-20
**Pipeline**: lok spec-review (synthesis/write steps failed on a NUL byte in reviewer output; this file reconstructed from the run log `/tmp/clo-487-spec-review.log`)

---

## 1. Problem Statement Assessment
Clear and well-defined. Contrasts with CLO-486, states the behavior gap, identifies the three correctness traps, references the locking ADR-001 decisions, and correctly marks out-of-scope (cache=CLO-491, full validation=CLO-492, provider trait=CLO-489). Matches the Linear description.

## 2. Acceptance Criteria Review
AC-1..AC-11 are specific, measurable, each with a verification method and a unit-vs-acceptance.sh approach.

**Gaps (minor)**:
- `--all`: clarify it wires to the existing CLO-486 single-commit path (skips grouping; no other functional change).
- Add a case for `groups: []` (empty array), distinct from empty group 1.
- Add a test row verifying the request payload includes `strict: true`.

## 3. Constraints Check
Aligned with existing patterns (shell-out git, blocking `ureq`, `GROQ_API_KEY`, NUL parsing + `quotePath=false`, index transaction, non-TTY guard, `--dry-run`/`--yes`, `$EDITOR`, exit codes).

**Concerns**:
- The NUL rename order is left to a test but the spec gives no expected example - add one for clarity (test remains authoritative).
- Resolve the "clear staged state" approach explicitly: existing `restore_index` uses `read-tree`, so `read-tree HEAD` to clear then `add -A -- <paths>` is consistent.
- Clarify per-file caps apply during assembly; `MAX_TOTAL_BYTES` is a final check on the assembled body (additional, not replacing).

## 4. Decomposition Quality
Well-scoped (~13h total, aligns with M/~7 files).

**Issues**:
- Sub-task 5 file list should also include `src/plan.rs` (validation) and `src/groq.rs` (`generate_plan`).
- Sub-task 2: confirm deletions stage via `git add -A -- <deleted-path>`; add a delete-only group test.
- Sub-task 3: prefer a new `GroupingContext` struct over extending `GatheredDiff` to avoid mixing concerns.

## 5. Evaluation Coverage
Comprehensive AC→test mapping.

**Missing scenarios**:
- Single-group plan (everything in one commit via the grouping path).
- `commit_message: null` in group 1 (the exact bash bug) → must fall back, not silently single-commit (Medium).
- HTTP timeout during the structured-output call (Low; hard to script - consider a unit test).

## 6. Codebase Alignment
No violations. Confirms cited references: `main.rs:69-85` `commit_flow`, `git.rs:45` quotePath, `git.rs:123-140` untracked `-z`, `git.rs:143-151` snapshot/restore, `diff.rs:91-100` tail-chop (what per-file caps replace), `groq.rs:132-168` strip_think.

## 7. Blind Spots
- **Grouping prompt content (Medium)**: specify the exact grouping system prompt - what the model receives (file list, porcelain status, diff stat, per-file diffs) and the return shape.
- **JSON Schema for `Plan` (Medium)**: include the concrete schema object sent with `json_schema` - critical for `strict: true`.
- **Fallback error message (Low)**: give example text, e.g. "Plan validation failed: group 1 references unknown file 'foo.txt'. Falling back to single-commit mode."
- **Concurrency (Low)**: note the change set is captured once at start; stale plan paths trigger fallback (AC-6).
- **`--all --dry-run` (Low)**: clarify it prints the single-commit message and exits (no grouping/staging/commit).
- **Single changed file (Very Low)**: single group → one commit; add a test row.

## 8. Verdict
**APPROVE_WITH_SUGGESTIONS**

Thorough, ADR-001/CLO-486 aligned, properly scoped; all ACs testable. Suggestions are improvements, not blockers.

## 9. Actionable Feedback
**Priority 1 (before implementation)**:
1. Define the JSON Schema for `Plan` (required props, `additionalProperties: false`, nullable `commit_message`).
2. Define the grouping system prompt (port/adapt from the bash tool).
3. Clarify `--all` (and `--all --dry-run`) behavior = existing single-commit path.

**Priority 2**: 4. `groups: []` test row. 5. single-group test row. 6. fallback message format. 7. resolve clear-index approach (`read-tree HEAD`).

**Priority 3**: 8. document NUL rename order. 9. clarify `MAX_TOTAL_BYTES` vs per-file caps.
