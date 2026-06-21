# Spec Review: clo-492

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-21
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and accurate. It perfectly matches the Linear task description, referencing FR-23 (bijective validation), FR-24 (single-commit fallback), FR-46 (index warning), and FR-47 (transactional commit). It grounds itself in the existing codebase by pointing out specific files and line numbers (e.g., the partial checks in `plan::validate_basic` in `src/plan.rs:243` and its doc-comment deferrals; the fallback routing comment in `src/main.rs:130`). There are no unstated assumptions; it correctly cites decisions from the locked architectural document (`ADR-001`).

## 2. Acceptance Criteria Review
**Strong**: 
- **Specific and Measurable**: All 10 ACs (AC-1 to AC-10) are specific, unambiguous, and have concrete verification conditions.
- **Robust Edge-Case Coverage**: Covers deterministic validation immediately falling back to single-commit (AC-1, AC-2, AC-3), the preservation of the CLO-487 grouping happy path (AC-4), index transactional integrity on decline (AC-5), clear and specific reasons on stderr warning (AC-6), and preventing transient error retries on deterministic failures (AC-8).
- **Unit Testing Pure Functions**: AC-9 explicitly specifies unit testing the pure validation function over a comprehensive array of edge cases (omissions, duplicates, empty groups, and `Display` distinctness) to enforce a high craft bar.

**Gaps**:
- **AC-2 Specificity**: While AC-1 explicitly mandates that the fallback reason must name the *omitted* file, AC-2 doesn't explicitly mandate that the duplicate reason must name the *duplicated* file in its main criteria block, though this is successfully specified under Section 3 (Constraints). Explicitly stating this in AC-2 would align it with AC-1.

## 3. Constraints Check
**Aligned**:
- **Zero-Dependency & Sync Constraints**: Properly mandates no new crate dependencies, async runtimes, or logging frameworks (fully aligned with ADR-001 Decisions 2 and 5).
- **Tolerant Schema Matching**: Correctly specifies that `groups[0]` carrying a message is the only message-placement check (null messages for later groups are tolerated/ignored to avoid spurious model fallbacks), which aligns with the regenerate-per-group contract (ADR-001 Decision 6).
- **Non-blocking warning**: Explicitly mandates that the curated-index warning is purely informational and must not block `--yes`/non-interactive runs or add a separate prompt, which maintains usability in CI/script contexts.

**Concerns**:
- No contradicting or missing constraints were found. The pure helpers specified for `ChangedFile` (status checking of `x` and `y`) are mathematically sound for the `git status --porcelain=v1` machine format, correctly isolating staged-only (`M `) vs. unstaged (` M`) and partial/hunk-level (`MM`/`AM`) states.

## 4. Decomposition Quality
**Well-scoped**:
- **Clean Logical Breakdown**: The division into 5 sub-tasks is excellent. It separates pure logical leaves (Sub-task 1 for the validator, Sub-task 2 for the git state helpers) from integration wiring (Sub-task 3 for validator wiring, Sub-task 4 for curated index warning, and Sub-task 5 for E2E acceptance tests).
- **Parallelizability**: The independent leaf nature of Sub-tasks 1 and 2 allows them to be developed in parallel, reducing developer friction.
- **Accurate Scope Estimation**: Each sub-task is well under the 2-hour scope limit.

**Issues**:
- No issues identified. The decomposition is highly logical and follows optimal dependency-ordered sequencing.

## 5. Evaluation Coverage
**Covered**:
- **1-to-1 Mapping**: The 17-row evaluation table maps perfectly to every single acceptance criterion.
- **Methodological Accuracy**: Uses unit testing for the pure business logic in `validate` and git status parsing, while relying on the existing python-based E2E mock-Groq server in `scripts/acceptance.sh` for integration validation.
- **Comprehensive Edge Cases**: Explicitly defines and details complex edge cases including renames (mapping the new path correctly), duplicates within the same group, single-group plans, and pre-commit hook transaction boundaries.

**Gaps**:
- No gaps found. The testing scenarios are remarkably exhaustive and leave no blind spots.

## 6. Codebase Alignment
**Violations**:
- None. The proposed implementation respects all architectural boundaries and error taxonomies.

**Alignment**:
- **Error Taxonomy Compatibility**: Incorporates specific `PlanError` variants which map cleanly into the `BuildError::Fallback(reason)` pattern in `src/main.rs:95-98`, producing highly detailed, specific stderr fallback warning reasons.
- **Git Porcelain Compatibility**: Directly leverages the NUL-delimited status parsing in `src/git.rs` to construct the `HashSet` change set, protecting filenames with special characters (spaces, arrows, and glob metacharacters).

## 7. Blind Spots
- **Future Automation Surface (`--plan-only`)**: `--plan-only` is a planned non-interactive flag for `CLO-493` (backlog) which performs no index mutations. The spec correctly excludes `--dry-run` from triggering the curated-index warning. However, once `--plan-only` is introduced, it should also be bypassed from triggering the curated-index warning to avoid unnecessary warnings during preview-only actions.

## 8. Verdict
```
APPROVE_WITH_SUGGESTIONS
```

## 9. Actionable Feedback
1. **Future-Proof Curated Index Warning (`--plan-only` bypass)**: Add a note to Sub-task 4 (and the main constraints) stating that the curated-index warning must only fire on the *real* execution paths, and should eventually bypass any future read-only automation flags such as `--plan-only` (just as it does for `--dry-run`).
2. **Explicit Duplicate Reporting in AC-2**: Update **AC-2**'s text to match AC-1's symmetry by explicitly mandating that the fallback reason must name the duplicated file (e.g., *"A plan that lists the same file in two groups is rejected to fallback with a reason naming the duplicated file"*).
3. **Document Unmerged-to-Staged Interaction**: Add a brief comment in `src/main.rs` near the curated-index check clarifying that unmerged conflicts (which would technically return `true` for `is_staged()`) are safely intercepted by the upstream `is_unmerged()` abort guard, guaranteeing that index warnings are only emitted on clean, mergeable repositories.
