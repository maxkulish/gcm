# Spec Review: clo-555

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-07-12
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Strengths:**
- The problem statement is clearly defined and self-contained, with specific file:line references to the existing defects
- It correctly identifies all four adjacent defects (unsafe parser, pre-confirmation mutation, uncentralized staging, remote Partial hazard)
- It matches the Linear task description precisely
- The design source of truth is explicitly referenced with the owner decisions documented

**Concerns:**
- None significant. The problem statement is thorough and actionable.

## 2. Acceptance Criteria Review

**Strong:**
- **AC1 (transaction success)**: Measurable - checks index, MERGE_HEAD, commit parents, signature, exit state
- **AC2 (abort restores)**: Byte-exact restoration is explicitly testable
- **AC3 (safe parser)**: Comprehensive coverage of cases (y/yes/Y, n/no/NO, e/edit, unknown input, EOF, empty Enter)
- **AC6 (signed finish)**: Postcondition-based classification is testable
- **AC12 (hygiene)**: Clear pass/fail criteria with clippy/fmt checks

**Gaps:**
- **AC5 (escalation semantics)**: Does not specify what happens when escalation occurs on the *first* file before any confirmation - should it still stage nothing and report Partial? The design doc clarifies "stage confirmed work", but if no work is confirmed yet, the behavior should be explicit
- **AC9 (truthful output)**: Does not specify JSON field behavior for `restored: bool` when status is not `aborted` (should be `false` or omitted via `skip_serializing_if`?)
- **AC10 (escape hatch)**: Does not specify whether `--no-finish` implies `--yes` for non-interactive use, or if it still requires interactive confirmation
- **AC11 (docs match runtime)**: References specific README lines but doesn't mention the `[conflict]` configuration table in README which may need updating for the new behavior

## 3. Constraints Check

**Aligned:**
- Snapshot/restore via byte-level IO aligns with existing `snapshot_index`/`restore_index` pattern in `src/git.rs:215-223`
- The `GIT_LITERAL_PATHSPECS=1` guard is already used in `stage_group` (`src/git.rs:418`)
- The `commit_signed` subprocess pattern with inherited stdin is established in `src/git.rs:230-241`
- No new dependencies aligns with the project's minimal dependency philosophy
- JSON fields with `skip_serializing_if` follows existing pattern in `src/resolve/report.rs`

**Concerns:**
- **Constraint "No change to `--dry-run` behavior"**: The spec says "no change" but the decomposition ST4 mentions "Mergiraf results become proposals; dead dry-run arm removed." This appears contradictory - need clarification on whether `--dry-run` path changes
- **Constraint on staging "never `git add -A` locally"**: The remote path in `src/resolve/remote/mod.rs:162` uses `git add -A` - while the constraint correctly limits this to remote, it's worth noting this existing pattern as a reference point
- **Missing constraint**: The spec doesn't explicitly constrain timing of staging relative to the finish subprocess - what happens if staging succeeds but finish crashes (e.g., power failure)? This is a cross-cutting concern

## 4. Decomposition Quality

**Well-scoped:**
- ST1 (shared prompt parser) is appropriately small and pure/unit-testable
- ST2 (byte snapshot/restore) is well-contained in `git.rs`
- ST3 (finish helper) is a focused addition with clear postconditions
- ST6 (remote gating) is a targeted fix to the existing remote wrapper

**Issues:**
- **ST4 (three-phase transaction)**: This sub-task is significantly larger than the others - it involves restructuring the entire `resolve_file` flow. At ~200 lines of core logic in `src/resolve/mod.rs` plus coordination with proposal collection, this could easily exceed 2 hours. Recommend splitting into:
  - ST4a: Proposal collection (no mutation phase)
  - ST4b: Abort/restore logic with snapshot integration
  - ST4c: Central staging by disposition
- **ST5 (finish integration)**: Depends on ST4 completing, which creates a serial bottleneck. If ST4 takes longer, ST5 cannot start
- **Missing sub-task**: There's no explicit sub-task for updating `FileAction` enum to include `Rejected` (mentioned in constraints but not in decomposition)
- **Missing sub-task**: The "interactive `--json` preview to stderr" requirement (AC9, AC12) is not explicitly covered in the decomposition

## 5. Evaluation Coverage

**Covered:**
- Tests 1-14 map well to acceptance criteria
- Edge cases section is thorough (CRLF, no-trailing-newline, rebase behavior)
- The probe-gate pattern for signing tests follows established convention

**Gaps:**
- **Missing test for "git checkout -m style conflicts"**: AC6 mentions `NothingToFinish` for "no operation ref" case, but no test explicitly covers this
- **Missing test for concurrent file modification**: What if the user edits a file in another terminal while `gcm resolve` is in phase B (confirm)? The snapshot would be stale
- **Missing test for mergiraf path when mergiraf is on PATH but returns non-zero**: The spec mentions mergiraf failures, but doesn't specify how they affect the three-phase structure
- **Missing test for SIGINT/SIGTERM handling**: If the user Ctrl-C's during phase C (apply), what state is the repo left in?
- **Test 7 (escalation)**: Doesn't specify the exact command list that should be printed for the "next command"

## 6. Codebase Alignment

**Violations:**
- **None identified** - the spec correctly identifies existing patterns to follow

**Alignment:**
- Follows the `GcmError` enum pattern for new error variants (`FinishFailed`, `NothingToFinish`)
- Correctly references existing `Repo` methods (`is_merging`, `is_rebasing`, `is_cherry_picking`, `unmerged_files`)
- Follows the established test patterns (`temp_repo()` helper, `run_git` helper in `tests/resolve_integration.rs`)
- Uses the established JSON envelope pattern (`v` field, `skip_serializing_if`)

## 7. Blind Spots

**Critical:**
1. **Interrupt handling during phase C (apply)**: The spec describes a three-phase transaction (propose, confirm, apply) but doesn't address what happens if the process is interrupted (SIGINT, SIGTERM, crash) during phase C. If staging succeeds but finish fails, the staged state is "intact" per AC7 - but if the process crashes *between* staging and finish, is there a cleanup/recovery path?

2. **Concurrent modification detection**: The snapshot is taken at the start of the run. If a user modifies a file in another terminal during the confirm phase, the abort restore would overwrite their new changes. The spec should consider adding a mtime/hash check before restore, or document this limitation

3. **Provider timeout during proposal building**: If a provider call times out during phase A (propose), the spec doesn't clarify whether this counts as an escalation or an error. The existing code has retry logic, but the three-phase structure changes when retries can occur

4. **Validation retry in three-phase context**: The existing code has validation retry logic (attempt_validation_retry in `resolve/mod.rs`). In the three-phase structure, does retry happen during phase A (potentially causing multiple provider calls), or is the file just escalated?

5. **Marker-free unmerged files already staged**: AC4 says marker-free files "are staged in the apply phase without a prompt" - but what if they were already staged before the run? The spec should clarify whether `git add` is idempotent or if there's special handling

**Moderate:**
6. **The `--yes` flag semantics**: AC3 mentions `--yes` for accepting all, but the interaction with `--no-finish` is unclear. Can you use both? What happens?

7. **JSON schema stability**: AC9 says `v` stays `1` but adds new fields. While `skip_serializing_if` handles absent fields, consumers may still break if they don't follow "ignore unknown fields" guidance

8. **Remote scratch path on abort**: When local resolve aborts, the working tree is restored. When remote resolve aborts, does the scratch repo get cleaned up, or preserved for debugging?

**Minor:**
9. **README line references**: AC11 references specific line numbers (e.g., "line ~181"). Line numbers drift over time - recommend referencing section headers or anchor text instead

10. **Test probe for signing**: The spec mentions "scripts/acceptance.sh probe_signing" - this file should be checked to exist or created as part of ST8

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is thorough and well-aligned with the codebase patterns. The core design is sound. The concerns are addressable clarifications and decomposition improvements rather than fundamental flaws.

## 9. Actionable Feedback

### Critical (resolve before implementation):

1. **Clarify interrupt handling**: Add a constraint or acceptance criterion describing what state the repository is left in if the process receives SIGINT during phase C (after staging but before finish completes). Consider adding SIGINT handler that reports "staged but not finished" state.

2. **Split ST4 into smaller sub-tasks**: Break ST4 into:
   - ST4a: Proposal collection (introduce `FileProposal` type, collect without mutation)
   - ST4b: Abort/restore with snapshot integration
   - ST4c: Central staging by final disposition
   This makes the implementation tractable within ~2 hour windows.

3. **Clarify `--yes` and `--no-finish` interaction**: Add explicit behavior for `--yes --no-finish` combination. Is `--no-finish` valid with interactive mode, or only with `--yes`?

### Moderate (address during implementation):

4. **Add mtime check before restore**: Consider checking if any file's mtime changed between snapshot and restore. If so, warn the user that their concurrent edits will be overwritten. Alternatively, document this limitation explicitly in the README.

5. **Specify validation retry timing**: Clarify in constraints whether validation retry happens during phase A (propose) or whether failed validation immediately escalates in the three-phase context.

6. **Add test for "no operation ref" case**: Test case for `git checkout -m` style conflict that has no `MERGE_HEAD`, `REBASE_HEAD`, or `CHERRY_PICK_HEAD` - verify `NothingToFinish` behavior.

7. **Add `FileAction::Rejected` sub-task**: Create an explicit sub-task for adding the new enum variant and updating the report serialization.

### Minor (nice to have):

8. **Reference README sections, not line numbers**: Change AC11 references from line numbers to section headers (e.g., "Resolve pipeline step 5", "Safety guarantees section").

9. **Create probe_signing script as part of ST8**: Ensure `scripts/acceptance.sh` with `probe_signing` function exists or is created.

10. **Add concurrent modification warning to constraints**: Document that concurrent file modifications during confirm phase will be overwritten on abort.
