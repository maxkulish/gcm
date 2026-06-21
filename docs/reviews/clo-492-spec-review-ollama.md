# Spec Review: clo-492

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-21
**Pipeline**: lok spec-review

---

Now I have a complete picture of the specification, related documents, and source code. Let me provide my comprehensive review.

---

## 1. Problem Statement Assessment

**Strengths:**
- The problem statement is **excellent** - it clearly identifies three specific gaps with precise code references (`src/plan.rs:243`, `src/main.rs:130`)
- It correctly maps each gap to specific FRs (FR-23, FR-24, FR-46, FR-47) and ADR decisions
- The "Who is affected / What triggers it / Why it matters" framing is clear and actionable
- It explicitly scopes what this slice does **not** touch (CLO-489, CLO-491, CLO-488), preventing scope creep

**Minor observation:** The statement references the bash validator (`docs/tmp/git-commit-ai.sh:382`) as evidence that coverage was never checked. This historical context is useful but could note that the referenced file may not be version-controlled (it's in `tmp/`).

---

## 2. Acceptance Criteria Review

**Strong:**
- AC-1 through AC-3 (omission, duplicate, empty group → fallback) are **specific, measurable, and testable**
- AC-5 (decline leaves index byte-identical) references concrete verification commands (`git status --porcelain=v1 -z`, `git write-tree`)
- AC-6 (warn-why on fallback) specifies the warning must include the specific `PlanError` reason
- AC-7 (curated-index warning) correctly distinguishes `--dry-run` (no warning) from actual runs
- AC-8 (retry timing) specifies request-count assertions which are objectively verifiable
- AC-9 (pure validator, unit-tested) lists exact test scenarios
- AC-10 (quality gates) references specific commands

**Gaps:**
1. **AC-7 lacks specificity on "when" the warning prints.** The spec says "before the index is reset" but doesn't specify if it's before or after the plan is displayed. This matters for UX - should the user see the warning before deciding to view the plan? The decomposition clarifies this (after unmerged guard, before `--all`/grouping branch), but AC-7 should reference the exact location.

2. **No acceptance criterion for the warning's exact wording.** AC-7 says the warning "states that the curated index will be reset and that partial (hunk-level) staging is not preserved in v1" but doesn't require an exact phrase or testable substring. The Constraints section suggests reusing `EGRESS_DISCLOSURE` wording, but AC-7 should require verifiable output.

3. **AC-5's "byte-identical" assertion may be too strict.** Git's `git status --porcelain=v1 -z` output is stable, but `git write-tree` produces a tree SHA. Testing that the SHA is identical before and after is correct, but "byte-identical" might be read as requiring binary comparison of the entire `.git/index` file. Recommend clarifying to "same tree SHA" or "same staged state."

**Suggested additions:**
- Consider adding an AC for what happens when both a curated index exists **and** the plan fails validation - does the user see both warnings? (The spec implies yes - fallback warning then curated-index warning on the single-commit path)

---

## 3. Constraints Check

**Aligned with codebase patterns:**
- The constraint to keep the validator **pure and deterministic** matches `plan::validate_basic`'s current signature (`&Plan, &HashSet<String> -> Result<(), PlanError>`)
- The constraint to not change `leaves_staged()` semantics matches the existing `GcmError::leaves_staged()` implementation
- The constraint to use `std::thread::sleep` matches ADR-001 Decision 2 (blocking, no async)
- The staged-state helpers `is_staged()` and `is_partially_staged()` follow the pattern of methods on `ChangedFile` (see existing `is_unmerged()`, `stage_paths()`)

**Concerns:**
1. **`PlanError::EmptyGroup(usize)` uses 0-based index internally but renders 1-based in `Display`.** This is consistent with the spec's stated intent, but the implementation should be explicit about this. The current `PlanError::EmptyFirstGroup` (singular) becomes `EmptyGroup(0)`. The spec correctly notes this, but should flag that **all `EmptyFirstGroup` references must be updated** - currently there's one usage in `validate_basic`.

2. **The constraint "Must not re-request the grouping call on a validation failure"** is correct, but the spec should note that retries are **already handled upstream** by CLO-488's `retry_with` in `groq::send_chat`. The current `generate_plan` in `src/groq.rs` wraps `send_chat` which already includes retries - so validation failure returns immediately without re-requesting. This is correct behavior but worth an explicit note in the spec.

3. **The `is_staged()` definition uses porcelain status codes.** The spec defines `is_staged` as `x != b' ' && x != b'?'`. This is correct for the XY porcelain format where `x` is the index status. However, looking at `ChangedFile` in `git.rs`:
   ```rust
   pub struct ChangedFile {
       pub x: u8,  // XY status - first char (index)
       pub y: u8,  // XY status - second char (worktree)
       pub path: String,
       pub orig_path: Option<String>,
   }
   ```
   The spec's definition is accurate, but should clarify that `x` is the **index status** (first character of XY), not the worktree status.

4. **The `is_partially_staged()` definition is `x != b' ' && x != b'?' && y != b' ' && y != b'?'`.** This correctly identifies `MM` (staged with additional changes) and `AM` (added with modifications). However, the spec should clarify what "partially staged" means in this context - it's specifically "staged AND has additional unstaged changes," which is the signature of `git add -p` workflow.

---

## 4. Decomposition Quality

**Well-scoped:**
- Sub-task 1 (full partition validator) is ~2 hours given the existing `validate_basic` foundation
- Sub-task 2 (staged-state helpers) is ~1 hour (simple boolean methods)
- Sub-task 3 (validator wiring) is ~1 hour (straightforward integration)
- Sub-task 4 (curated-index warning) is ~2 hours (requires checking all paths)
- Sub-task 5 (acceptance tests) is ~2 hours (extends existing harness)

**Dependencies are correctly identified:**
- 1 and 2 are independent leaves ✓
- 3 depends on 1 ✓
- 4 depends on 2 ✓
- 5 depends on 1-4 ✓

**Missing sub-task:**
- **Documentation update** - The spec mentions updating `README.md` / `--help` text in sub-task 5, but this should be explicit. The existing `EGRESS_DISCLOSURE` in `src/cli.rs:6` should be checked for consistency with the new runtime warning.

---

## 5. Evaluation Coverage

**Covered:**
- All 10 ACs have corresponding test cases
- Edge cases like "omission with a rename" and "all files in one group" are included
- The `--yes` + curated-index combination is tested (AC-7 variant)

**Gaps:**
1. **No test for "plan has files but change_set is empty."** The spec correctly notes this can't occur (the guard at `main.rs:50` exits before validation), but it's worth a unit test to assert the invariant.

2. **No test for "validation fails AND curated index exists."** What happens when `is_staged()` returns true AND `validate_basic` fails? The spec implies both warnings print (curated-index warning before the fallback, then the fallback warning), but this isn't explicitly tested.

3. **Test #17 (request count) assumes `--all` path.** The spec correctly notes `--all` isolates one request, but for the grouping path, validation failure comes after the plan call + potentially one fallback message call. The test should specify whether it's testing `--all` or grouping path.

4. **Missing test for "plan with duplicate file in SAME group."** The spec mentions this edge case ("the same path in the **same** group twice is also a duplicate"), but it's not in the test table. The `HashSet`/`HashMap` approach will catch this, but a test case is warranted.

---

## 6. Codebase Alignment

**Violations:**
1. **The spec proposes renaming `validate_basic` to `validate`.** However, `src/plan.rs` already has a test `accepts_a_valid_plan` that calls `validate_basic`. The spec should note that **all existing test references to `validate_basic` must be updated**.

2. **The `EmptyFirstGroup` variant must be removed or deprecated.** The spec correctly notes this should be generalized to `EmptyGroup(0)`, but the transition must be explicit:
   - Update `PlanError::EmptyFirstGroup` → `EmptyGroup(0)`
   - Update the `Display` impl
   - Update any pattern matches (currently in `validate_basic`)

3. **The `src/main.rs:96` fallback warning location may be incorrect.** Looking at the current code:
   ```rust
   Err(BuildError::Fallback(reason)) => {
       eprintln!("gcm: {reason}. Falling back to single-commit mode.");
       return single_commit(&repo, args);
   }
   ```
   This is inside `execute()` after the `build_plan` call. The curated-index warning (AC-7) must print **before** this fallback branch is reached. The spec correctly places it "after the unmerged-conflict guard and before the `--all`/grouping branch," but should verify this matches `main.rs` line numbers (the current code shows the fallback at ~line 96 in the match on `cache::load`).

**Alignment:**
- The `GcmError::leaves_staged()` pattern matches the existing `CommitFailed` handling
- The `BuildError` enum (Fatal/Fallback) is correctly reused for the new validation errors
- The `Decision::Abort` path correctly returns `Ok(())` for exit code 0
- The `snapshot_index` / `restore_index` pattern in `src/git.rs` is correctly referenced

---

## 7. Blind Spots

1. **What happens if `ChangedFile::is_staged()` returns true but the user **runs with `--yes`**?** The spec says the warning "prints and proceeds," but should this be silent in `--yes` mode? A user running `--yes` has opted into automation and might not want informational warnings. The spec doesn't address this. **Recommendation:** Consider whether curated-index warnings should be suppressed with `--yes`.

2. **The spec doesn't address `--dry-run` + curated index.** AC-7 says "`--dry-run` prints no such warning," which is correct (nothing is mutated). But should a `--dry-run` user be informed that their curated index would be reset? This could be valuable information. **Recommendation:** Consider whether `--dry-run` should print a different informational message like "Note: running without --dry-run would reset your staged changes."

3. **No discussion of `--reset` + curated index.** If `--reset` is passed (clearing the cache), should the curated-index warning still fire? `--reset` doesn't affect staging, so yes - but this should be explicit.

4. **The `OmittedFile` error could produce a very long message** if many files are omitted. A plan that omits 50 files would produce `"plan omitted changed file 'a.rs'"` for only the **first** omitted file (per the spec's check order), which is correct but the user must re-run N times to discover all omissions. This is acceptable (fail-fast) but worth noting.

5. **The spec doesn't address what happens if `plan::validate` is called with an **empty** change_set.** The spec correctly notes this can't happen (guarded earlier), but the function signature `validate(&Plan, &HashSet<String>)` should either panic on empty or return `Ok(())` for a plan with no groups. The current `validate_basic` returns `NoGroups` for empty groups, which is correct.

6. **The relationship between FR-46 (Should) and the runtime warning isn't clear.** FR-46 is a "Should" (not "Must"), but the spec treats it as required. This is correct (ADR-001 Decision 9 confirms), but the spec should explicitly cite ADR-001 Decision 9 as the binding requirement.

7. **No discussion of how `orig_path` (renames) interact with the validation.** The spec notes that renames contribute the new path to `change_set`, and the validator uses `path` only. This is correct, but the test table should include a rename in the change_set to verify this explicitly.

---

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is well-structured, aligns with the codebase, and correctly identifies the implementation changes. The core logic (partition validation, staged-state helpers, warning emission) is sound. However, there are minor gaps in test coverage for edge cases and some clarity improvements needed around warning ordering and word choices.

---

## 9. Actionable Feedback

**Priority 1 (Must fix before implementation):**

1. **Add missing test case for "duplicate in same group"** - Test #2 in the table tests cross-group duplicate, but same-group duplicate (e.g., `"files": ["a.rs", "a.rs"]`) is not explicitly tested.

2. **Clarify AC-7 timing relative to fallback.** The AC should state: "The curated-index warning prints after the unmerged-conflict guard and before any plan-generation or staging, including before the fallback warning."

3. **Add explicit test for validation failure + curated index.** Both conditions simultaneously should produce both warnings in the correct order.

**Priority 2 (Should address):**

4. **Add AC for exact warning substring.** AC-7 should require verifiable output, e.g.: "The warning must contain 'curated index', 'reset', and 'hunk-level staging is not preserved'."

5. **Clarify `--yes` behavior with curated index.** Should the warning still print when the user has opted into automation? Consider adding a note in Constraints.

6. **Add the ADR-001 Decision 9 citation** to the FR-46 Must constraint to establish precedence.

**Priority 3 (Nice to have):**

7. **Update sub-task 1 to explicitly note removing `EmptyFirstGroup`** and updating all references.

8. **Consider a test for "all files in one group"** - a single-group plan covering every changed file should pass validation (it's just "group everything").

9. **Add a note about `--reset` + curated index** interaction.

10. **Consider adding a constraint note about the relationship between CLO-488 retries and validation.** The spec correctly notes retries happen upstream, but this should be explicit in a "Must-not" or "Prefer" constraint.
