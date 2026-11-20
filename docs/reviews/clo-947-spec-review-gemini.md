# Spec Review: clo-947

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Reviewed**: 2026-11-20
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is clear, self-contained, and matches the Linear task description for CLO-947. Every file path, line number, and move count verified against the repository is exact:

- `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/spec-review.toml` (lines 354-357, 4 moves)
- `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/design-review.toml` (lines 395-398, 4 moves)
- `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/pre-pr-validation.toml` (lines 49, 399, 434-437, 6 moves)
- `/Users/mk/Code/gcm--fix-clo-947-report-publish/.claude/commands/task/phases/implement.md` (lines 69, 133, 3 moves on 2 lines)

Total: 17 `mv -f` moves across 16 lines, with no other `mv` invocations in those four files. The failure propagation analysis and the `lok 20261107.0.2` stderr probe directly justify why `REVIEW_PUBLISH_FAILED:` must go to stderr.

One unstated assumption to verify: the probe ran `lok run --verbose`, whereas `/Users/mk/Code/gcm--fix-clo-947-report-publish/.pi/orchestrator/phases/implement.md` (line 218) invokes `lok workflow run pre-pr-validation ...` without `--verbose`. Whether `lok` without `--verbose` also prints failed-step stderr after `✗` is unstated.

## 2. Acceptance Criteria Review

**Strong**:
- AC1-AC4 define exact POSIX `sh` function signatures, exact `REVIEW_PUBLISH_FAILED: <target>: <reason>` error strings, stdout/stderr isolation, file state invariants, and exact wiring rules across all 17 call sites.
- AC6-AC8 cleanly integrate directory failures with the existing `REVIEW_INPUT_FAILED:` path (diff sites), `REVIEW_PUBLISH_FAILED:` exit 1 path (report sites), and normal-case behavior.
- AC9-AC10 explicitly require cross-platform CI execution (`ubuntu-latest` and `macos-latest`) and pinpoint the four reader documentation locations that need updating.

**Gaps**:
- **AC5 step name inaccuracy**: AC5 states "the `write_reviews` step exits 1", which matches `spec-review.toml` and `design-review.toml`, but in `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/pre-pr-validation.toml` (line 340) the step is named `write_reports`.
- **AC3 second branch vs. AC9**: AC3 requires `not a regular file after the move` when `mv` exits 0 but `<target>` is not a regular file afterwards, and AC9 states `scripts/test-review-guard.sh` covers AC1-AC4. However, neither Decomposition #2 nor Evaluation #1-#6 includes a test case for this second AC3 branch.
- **AC3 empty `mv` stderr edge case**: AC3 specifies `mv failed: <first line of the mv stderr>` without specifying a fallback if `mv` fails with empty stderr (which would leave a trailing `: `).

## 3. Constraints Check

**Aligned**:
- Enforces POSIX `sh` compatibility in `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/lib/review-guard.sh`, avoiding GNU-only flags (`mv -T` / `mv --no-target-directory`) so macOS BSD `mv` and Linux GNU `mv` behave identically.
- Preserves the CLO-945 named-failure prefix convention and leaves existing stdout failure tokens (`NO_REVIEWS_AVAILABLE`, `SYNTHESIS_FAILED`, `REVIEW_INPUT_FAILED`) untouched as a separate follow-up.
- Correctly preserves the prefix-stripping pattern at both diff sites (`PUB=$(review_publish "$DIFF.tmp" "$DIFF" 2>&1)`), preventing extra stdout lines in `health_check` that would interfere with `gemini=`/`codex=`/`input=` status parsing.
- Commits `scripts/test-review-guard.sh` as executable and runs it via `sh` in CI, matching the pattern of `/Users/mk/Code/gcm--fix-clo-947-report-publish/scripts/check-public-surface.sh` and `.github/workflows/ci.yml`.

**Concerns**:
- None contradicting codebase patterns. Note that in Constraints line 64, `${PUB#"REVIEW_PUBLISH_FAILED: $DIFF: "}` relies on `$DIFF` containing no shell pattern metacharacters and double quotes suppressing globbing in POSIX `sh`; `review_valid_slug` ensures `$SLUG` is restricted to `[A-Za-z0-9._-]`, so this is safe under both `dash` (`ubuntu-latest`) and `sh` (`macos-latest`).

## 4. Decomposition Quality

**Well-scoped**:
- Five independent, ~S-sized sub-tasks touching 10 unique files, with clear dependency ordering (Sub-task 1 blocks 2, 3, and 4; Sub-tasks 3, 4, and 5 can be implemented in parallel; Sub-task 2's wiring check passes once 3 and 4 land).

**Issues**:
- **Sub-task 2 (`scripts/test-review-guard.sh`) omits the AC3 post-move check**: Add a behavior case where `<src>` is a directory (or mock `mv`) so `mv -f` exits 0 while `<target>` is not a regular file afterwards, verifying the `not a regular file after the move` reason.
- **Sub-task 2 wiring count subtlety**: `/Users/mk/Code/gcm--fix-clo-947-report-publish/.claude/commands/task/phases/implement.md` line 133 has two `review_publish` calls on one line (`review_publish A && review_publish B || exit 1`). Counting matching lines with `grep -c` would return `2` instead of the expected `3`; the script must count occurrences (e.g., `grep -o 'review_publish' ... | wc -l` or `awk`).

## 5. Evaluation Coverage

**Covered**:
- Tests #1-#7 cover unit behavior of `review_publish` (directory, symlink to directory, missing target, regular file replacement, read-only parent failure), static wiring checks, and both CI OS runners.
- Tests #8-#15 cover end-to-end `lok run` and inline gate failure paths, normal-case regression across all four gates, and documentation updates.

**Gaps**:
- **Missing test row for AC3 post-move regular-file check**: No evaluation row verifies `REVIEW_PUBLISH_FAILED: <t>: not a regular file after the move`.
- **Missing `--verbose` clarity in #10 and #11**: Tests #8 and #9 explicitly pass `--verbose` to `lok run`, while #10 and #11 omit it. If `lok` only prints step stderr under `--verbose`, #11 needs `--verbose` (or an explicit check without `--verbose` if Pi relies on stderr visibility in `lok workflow run`).
- **Partial publication check in #13**: Test #13 should specify testing the second move (`$GR` as a directory while `$CR` is valid) or both moves, confirming `gate exit 1` and `REVIEW_PUBLISH_FAILED:` even after `$CR` succeeds.

## 6. Codebase Alignment

**Violations**:
- None. Note that this specification targets the review-gate shell/TOML/markdown subsystem (`.lok/`, `.claude/commands/`, `scripts/`, `.github/workflows/ci.yml`), not the Rust `src/` crate (`Provider`/`Backend` traits, `anyhow`, or `BackendErrorKind`), which aligns with CLO-945's constraint against touching Rust source for gate fixes.

**Alignment**:
- Matches `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/lib/review-guard.sh` style (POSIX `sh`, `_rp_*` local variable naming convention, header doc block listing function signature and behavior).
- Preserves the `verdict_gate` dependency pattern from CLO-945 lesson L3 (`.pi/lessons/clo-945-review-gates-lessons.md`) so `write_reviews`/`write_reports` non-zero exits fail the `lok` workflow.
- Keeps `.pi/` out of scope per CLO-945/947 boundaries while preserving the non-zero exit contract that `/Users/mk/Code/gcm--fix-clo-947-report-publish/.pi/orchestrator/phases/implement.md` (line 239) already checks.

## 7. Blind Spots

- **`[ -f "$target" ]` vs. symlinked `<src>`**: On POSIX `sh`, `[ -f "$target" ]` returns true for a symbolic link to a regular file. If `<src>` were ever a symlink rather than a regular file, `[ -f "$target" ] && [ ! -L "$target" ]` is stricter; since all callers stage into regular files, either is acceptable, though worth noting.
- **`lok` stderr visibility without `--verbose`**: Since `/Users/mk/Code/gcm--fix-clo-947-report-publish/.pi/orchestrator/phases/implement.md` (line 218) calls `lok workflow run pre-pr-validation` without `--verbose`, confirming whether `lok` without `--verbose` also displays failed-step stderr ensures Pi users see `REVIEW_PUBLISH_FAILED:` without needing `--verbose`.
- **Other `mv` forms in future edits**: Checking only `mv -f` in `scripts/test-review-guard.sh` misses plain `mv` if added later, though all 17 current sites use `mv -f`.

## 8. Verdict

APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback

1. **Add test coverage for AC3's post-move regular-file check** (Priority: High): In Decomposition #2 and Evaluation (Section 5), add a case in `scripts/test-review-guard.sh` where `<src>` is a directory (`mkdir`) and `<target>` does not exist, so `mv -f` exits 0 and `review_publish` returns non-zero with `REVIEW_PUBLISH_FAILED: <t>: not a regular file after the move`.
2. **Clarify step name in AC5** (Priority: Low): Update AC5 from "the `write_reviews` step exits 1" to "the write step (`write_reviews` or `write_reports`) exits 1" to match `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/pre-pr-validation.toml` (line 340).
3. **Note occurrence counting for `implement.md:133` in Decomposition #2** (Priority: Medium): Specify counting `review_publish` occurrences (e.g., via `grep -o` or `awk`) rather than `grep -c`, since `implement.md:133` holds two calls on one line.
4. **Verify `lok` stderr behavior without `--verbose` for #10/#11** (Priority: Medium): Confirm whether `lok run` / `lok workflow run` prints failed-step stderr without `--verbose`, or add `--verbose` to Evaluation #10 and #11 to match #8 and #9.
5. **Handle empty `mv` stderr in `review_publish`** (Priority: Low): In AC3 / Sub-task 1, fall back to `"exit $rc"` if `mv` exits non-zero with empty stderr so the error line never ends with a bare `"mv failed: "`.
