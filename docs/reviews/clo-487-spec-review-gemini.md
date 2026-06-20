# Spec Review: clo-487

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-20
**Pipeline**: lok spec-review (synthesis/write steps failed on a NUL byte in reviewer output; this file reconstructed from the run log `/tmp/clo-487-spec-review.log`)

---

## 1. Problem Statement Assessment
Exceptionally clear, complete, and accurate. Captures the transition from the CLO-486 single-commit tracer to semantic grouping via structured JSON outputs on Groq (`gpt-oss-120b`), matches the Linear task, and resolves the fragile scraping of the legacy bash script. Correctly isolates three technical challenges: Path Agreement, Diff Truncation structure preservation, Safe Degradation.

## 2. Acceptance Criteria Review
**Strong**: AC-1 & AC-2 (split + cache-less progression via re-analysis), AC-4 (NUL-safe path survival incl. ` -> ` in a name), AC-5 (per-file placeholders avoid tail-chop), AC-10 (write-tree/read-tree transaction).

**Gaps**:
- **Rename Staging Defect**: AC-4 asserts renames stage correctly, but staging only the new path via `git add -A -- <new_path>` leaves the deletion of the original path unstaged. Both paths must be staged together.
- **Interactive Editing during Grouping**: Make explicit that `e` edits only group 1's message for the current run; subsequent groups are re-analyzed next run.

## 3. Constraints Check
**Aligned**: Groq `strict: true` on `gpt-oss-120b` (ADR-001 #5); no cache (deferred to CLO-491); synchronous `ureq`.

**Concerns**:
- **Index Clearing on Unborn Branch**: `git read-tree HEAD` fails on an unborn branch (HEAD does not resolve). An explicit fallback to the empty-tree object `4b825dc642cb6eb9a0fa8e4ced7b1d4154961131` (or `read-tree --empty`) is required.

## 4. Decomposition Quality
Outstanding. Sub-tasks are granular, sequential, ~2-hour-scoped; new types in `src/plan.rs`, status parsing in `src/git.rs`, prompt diff in `src/diff.rs`. No issues.

## 5. Evaluation Coverage
Covered: nearly all ACs map to realistic unit + acceptance.sh (mock Groq) tests, mirroring CLO-486.

**Gap**: No explicit evaluation row validating grouping on an unborn branch (mentioned only in edge cases).

## 6. Codebase Alignment
No pattern violations. Respects the synchronous `Repo`/`groq` wrappers, the `GcmError` boundary, GPG signing, terminal inheritance, and `tempfile` handling.

## 7. Blind Spots
1. **Rename Staging Bug (Critical)**: A rename is an unstaged deletion of old + addition of new. Clearing the index and staging only the new path leaves the old-path deletion unstaged → the commit adds the new file but does not delete the old one, splitting the rename across commits. Solution: for a `Rename`, stage both paths: `git add -A -- <new_path> <orig_path>`.
2. **Tracked Diff Per-File Truncation (Minor)**: Per-file truncation is defined for untracked files (`PER_FILE_BYTES = 8192`) but vague for tracked diffs. Split the `git diff` string on `diff --git ` boundaries and cap each file's hunk body individually, appending `[diff omitted: N bytes]` — reuse the section-splitting logic in `elide_binary_diff`.

## 8. Verdict
**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback
1. **Fix the Rename Staging Logic (High)**: when a renamed file (`R`) is staged, pass both its current and original path to the staging command.
2. **Fix Index Clearing on Unborn Branches (High)**: use the empty-tree SHA / `read-tree --empty` fallback when there is no `HEAD`.
3. **Detail Tracked Diff Section Splitting (Medium)**: split tracked diffs on the `diff --git ` delimiter; cap individual hunk bodies at 8192 bytes, matching untracked logic.
4. **Explicitly Map Rename Status Parsing (Medium)**: document the `git status --porcelain=v1 -z` rename format; the parser must consume the paired path for `R`/`C`. (Note: Gemini's own text gave contradictory orderings - see synthesis; the empirical test is authoritative.)
5. **Add unborn-branch scenario to Evaluation Table (Low)**.
