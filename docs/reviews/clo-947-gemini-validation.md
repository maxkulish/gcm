# Pre-PR Validation: clo-947

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Validated**: 2026-11-20
**Pipeline**: implement.md validation gate

---

## Verdict: PASS

All 10 acceptance criteria (including AC3a) and all constraints in `/Users/mk/Code/gcm--fix-clo-947-report-publish/docs/specs/2026-11-20-clo-947-report-publish.md` are implemented accurately across all 20 changed files in `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/local/review-input/clo-947-branch.diff`.

### Summary of Verification
- **AC1-AC3a (`/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/lib/review-guard.sh`)**: `review_publish` is POSIX `sh`, checks `[ -d "$_rp_tgt" ]` before moving (refusing both directories and symlinks to directories without modifying source or target), invokes `mv -f --` and captures stderr so callers never see raw `mv` stderr, falls back to `exit $_rp_rc` when `mv` fails with empty stderr, verifies `[ -f "$_rp_tgt" ]` after the move, and routes the full `REVIEW_PUBLISH_FAILED: <target>: <reason>` line to `<record>` while writing only the 52-character classifier-safe line `REVIEW_PUBLISH_FAILED: see the run publish-failed file` to stderr (falling back to the full stderr line if appending to `<record>` fails).
- **AC4-AC8 (Workflows and Inline Gate)**: All 17 `mv -f` publish sites across `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/spec-review.toml` (4), `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/design-review.toml` (4), `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/pre-pr-validation.toml` (6: 1 diff site + 5 report sites including the `input-failed` synthesis publish), and `/Users/mk/Code/gcm--fix-clo-947-report-publish/.claude/commands/task/phases/implement.md` (3: 1 diff site + 2 inline report sites) are converted to `review_publish` with the required failure handling. Each lok write step runs `rm -f "$RUN/publish-failed"` before its first publish.
- **AC9 (`/Users/mk/Code/gcm--fix-clo-947-report-publish/scripts/test-review-guard.sh` and `/Users/mk/Code/gcm--fix-clo-947-report-publish/.github/workflows/ci.yml`)**: Executable (`100755`) POSIX `sh` test script covers every helper branch (directory, directory symlink, missing target, regular file replacement, read-only parent, silent `mv` failure, multi-line `mv` stderr, unwritable record fallback, directory as source, record file on failure and absence on success) and all static wiring counts/checks, wired into CI on `ubuntu-latest` and `macos-latest`.
- **AC10 (Reader Docs)**: All 7 places (`/Users/mk/Code/gcm--fix-clo-947-report-publish/.claude/commands/task/phases/spec.md`, `/Users/mk/Code/gcm--fix-clo-947-report-publish/.claude/commands/task/phases/design.md`, `/Users/mk/Code/gcm--fix-clo-947-report-publish/.claude/commands/design-doc/review.md`, `/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/pre-pr-validation.toml`, `/Users/mk/Code/gcm--fix-clo-947-report-publish/.pi/orchestrator/phases/spec.md`, `/Users/mk/Code/gcm--fix-clo-947-report-publish/.pi/orchestrator/phases/design.md`, and `/Users/mk/Code/gcm--fix-clo-947-report-publish/.pi/orchestrator/phases/implement.md`) document `REVIEW_PUBLISH_FAILED: see the run publish-failed file` and `publish-failed` before any rerun. The pre-PR docs also cover `input-failed`, and the Pi `implement.md` failure paragraph treats report files from a failed run as untrusted. Run-directory casing (`CLO-XX` verbatim for `spec-review`/`design-review` vs. lowercased `clo-xx` for `pre-pr-validation`) matches each workflow's `RUN=` definition.

---

## Findings

- **[LOW] Duplicate output line on symlink-replacement failure in test script**
  - **Where**: `/Users/mk/Code/gcm--fix-clo-947-report-publish/scripts/test-review-guard.sh:44-46`
  - **What**: `refused` prints `ok - target is a link to a directory` on line 45, and line 46 separately checks `[ -L "$C/target" ] || fail "target is a link to a directory" "link was replaced"`. If a regression ever replaced the symlink without triggering an earlier check in `refused`, the script would emit both an `ok -` and a `FAIL -` line for the same case name (though `FAILS` would still increment and exit 1).

- **[LOW] Wiring check `mv` pattern only catches arguments starting with `-`, `"`, or `$`**
  - **Where**: `/Users/mk/Code/gcm--fix-clo-947-report-publish/scripts/test-review-guard.sh:148`
  - **What**: `grep -nE '(^|[;&|({[:space:]])mv +[-"$]'` matches the regex specified in Decomposition #2 of the spec, which avoids false positives on English prose, but would not flag a future edit that writes `mv` with single-quoted or bare arguments (e.g., `mv 'a' 'b'`).

---

## Missing Items

None. All acceptance criteria (AC1-AC10 and AC3a) are covered.

---

## Recommendations

- Fold the `[ -L "$_t" ]` invariant check into `refused` (or gate it on an optional flag) in `/Users/mk/Code/gcm--fix-clo-947-report-publish/scripts/test-review-guard.sh:44-46` if strict one-line-per-case output on failure is desired.
- Confirm the `Review guard test` CI step passes on both `ubuntu-latest` and `macos-latest` once the PR checks run.
