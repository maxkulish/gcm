# Spec: Review report publication fails by name when the target path is a directory

**Created**: 2026-11-20
**Linear**: [CLO-947](https://linear.app/cloud-ai/issue/CLO-947)
**Estimated scope**: S (10 files, most of them one-line edits; 5 sub-tasks)

## 1. Problem Statement

The review gates from CLO-945 stage each report in a run directory and then publish it with `mv -f <staged> <target>`. When `<target>` is an existing directory, or a symbolic link to a directory, `mv` moves the staged file into that directory and exits 0 on both GNU and BSD `mv`. The gate continues as if the report were published. The next step then reads `<target>`, which is a directory, not the report.

The same pattern publishes the branch diff that the pre-PR reviewers read. If the diff target is a directory, `mv -f "$DIFF.tmp" "$DIFF"` succeeds, and the reviewers get a prompt that points at a directory.

Codex rated this HIGH in the PR #90 review. The owner deferred it to this issue.

Publish sites: 17 moves on 16 lines, all in the review-gate subsystem:

| File | Lines | What is published |
|---|---|---|
| `.lok/workflows/spec-review.toml` | 354-357 | gemini, codex, fallback, synthesis reports (4 moves) |
| `.lok/workflows/design-review.toml` | 395-398 | gemini, codex, fallback, synthesis reports (4 moves) |
| `.lok/workflows/pre-pr-validation.toml` | 49 | branch diff (`$DIFF.tmp` to `$DIFF`) |
| `.lok/workflows/pre-pr-validation.toml` | 399 | input-failed synthesis |
| `.lok/workflows/pre-pr-validation.toml` | 434-437 | gemini, codex, fallback, synthesis reports (4 moves) |
| `.claude/commands/task/phases/implement.md` | 69 | branch diff in the inline pre-PR gate |
| `.claude/commands/task/phases/implement.md` | 133 | codex and gemini reports in the inline pre-PR gate (2 moves on one line) |

How a failure reaches the reader today:

- In the three lok workflows, the `write_reviews` step exits 1 when a publish fails, and the `verdict_gate` step that depends on it makes `lok run` exit non-zero (CLO-945 lesson L3). The phase files already treat a non-zero lok exit as "reports not trustworthy". No line names the failure, though.
- In `pre-pr-validation.toml` and `implement.md`, a failed diff publish sets `WHY="cannot write $DIFF"`. This becomes `REVIEW_INPUT_FAILED: <reason>`. The path is correct, but `mv` never fails in the directory case, so the path never runs.
- In `implement.md`, a failed report publish exits 1 with no named line. The text after the block says only "a setup or write step failed".

What `lok run` shows of a failed step. This was probed on lok 20261107.0.2 with a two-step workflow: the first step prints one line to stdout and one to stderr, then exits 1, and the second step depends on it. `lok run --verbose` exited 1. It printed the failed step's stderr after `✗` and did not print its stdout. A named failure line from a lok step therefore must go to stderr, or the reader never sees it.

The shared gate library is `.lok/lib/review-guard.sh` (POSIX sh). All four gate files already source it.

## 2. Acceptance Criteria

- [ ] AC1: `.lok/lib/review-guard.sh` has a function `review_publish <src> <target>`. When `<target>` is a directory or a symbolic link to a directory, it leaves `<src>` and `<target>` unchanged. It writes `REVIEW_PUBLISH_FAILED: <target>: target is a directory` to stderr, writes nothing to stdout, and returns non-zero.
- [ ] AC2: When `<target>` does not exist or is a regular file, `review_publish` moves `<src>` to `<target>`, writes nothing to stdout or stderr, and returns 0. After it returns 0, `<target>` is a regular file with the content of `<src>`, and `<src>` no longer exists.
- [ ] AC3: When `mv` fails, `review_publish` writes `REVIEW_PUBLISH_FAILED: <target>: mv failed: <first line of the mv stderr>` to stderr and returns non-zero. When `mv` succeeds but `<target>` is not a regular file afterwards, the reason is `not a regular file after the move`. The `mv` stderr never reaches the caller except inside that one line.
- [ ] AC4: All 17 publish moves use `review_publish`. No `mv -f` remains in the four gate files. Each call has failure handling: `|| exit 1` at a report site, `elif ! ...` at a diff site, and `review_publish A && review_publish B || exit 1` on `implement.md:133`.
- [ ] AC5: Report publish in a lok workflow. If a report target is a directory, the `write_reviews` step exits 1 before it prints `usable=<N>`. The `lok run` output shows `REVIEW_PUBLISH_FAILED: <target>: target is a directory` after `✗`, and `lok run` exits non-zero.
- [ ] AC6: Diff publish when `$DIFF` is a directory.
  - lok (`pre-pr-validation.toml`): `health_check` sets `WHY` to `cannot write <DIFF>: target is a directory`. `write_reviews` then writes `REVIEW_INPUT_FAILED: cannot write <DIFF>: target is a directory` as the first line of `docs/reviews/<slug>-validation-synthesis.md`. No reviewer runs, and `lok run` exits non-zero.
  - Inline gate (`implement.md`): the block prints `REVIEW_INPUT_FAILED: cannot write <DIFF>: target is a directory` and exits 3. No reviewer runs.
- [ ] AC7: Report publish in the inline gate. If a report target is a directory, the block prints the `REVIEW_PUBLISH_FAILED:` line and exits 1. It prints no `codex=`, `gemini=` or `usable=` lines. The text after the block names `REVIEW_PUBLISH_FAILED:` as a cause of `gate exit 1`.
- [ ] AC8: The normal case does not change. With no file or a regular file at each target, all three workflows and the inline gate publish their reports to the same paths and print the same success lines as before.
- [ ] AC9: `scripts/test-review-guard.sh` covers AC1-AC4, including the wiring check in AC4. CI runs it on ubuntu-latest and macos-latest.
- [ ] AC10: The reader docs name the new line. These are the non-zero-exit paragraphs in `.claude/commands/task/phases/spec.md` (step 2c-post item 1), `.claude/commands/task/phases/design.md` (item 1 of the same check) and `.claude/commands/design-doc/review.md` (line 345), plus the header comment of `.lok/workflows/pre-pr-validation.toml` (lines 16-18). Each says that a `REVIEW_PUBLISH_FAILED: <target>: <reason>` line in the lok output names a report that was not published. The rule to read no report stays as it is.

**Verification method**: `scripts/test-review-guard.sh` for AC1-AC4 and AC9, run locally on macOS and in CI on both runners. Stub-reviewer runs of `lok run` and of the inline block for AC5-AC8 (see Evaluation). A read of the four doc paragraphs for AC10.

## 3. Constraints

**Must**:
- Keep `.lok/lib/review-guard.sh` POSIX sh. It runs under `sh` on macOS and on Linux.
- Detect the directory case before the move. `[ -d "$target" ]` follows symbolic links, so the same test refuses a link to a directory.
- Write the `REVIEW_PUBLISH_FAILED:` line to stderr only. lok shows the stderr of a failed step and drops its stdout (Problem Statement).
- Capture the `mv` stderr inside `review_publish` (for example `err=$(mv -f "$1" "$2" 2>&1)`), so that the caller gets exactly one line.
- Keep the named-failure style of CLO-945: one line, an upper-case prefix, a colon, then the reason.
- Keep the publish order and the `|| exit 1` control flow at each lok report site. Only the move command changes.
- Keep the diff failure on the existing `REVIEW_INPUT_FAILED` path. At both diff sites, capture the helper line and strip its fixed prefix:
  `elif ! PUB=$(review_publish "$DIFF.tmp" "$DIFF" 2>&1); then WHY="cannot write $DIFF: ${PUB#"REVIEW_PUBLISH_FAILED: $DIFF: "}"`.
  The helper line must not appear on its own in addition to the `REVIEW_INPUT_FAILED` line. In `health_check`, stdout carries the `gemini=` / `codex=` / `input=` status lines, so an extra line there breaks the status parsing.
- Make the test script exit non-zero on any failed case and print one line per case.
- Run the test from CI as `sh scripts/test-review-guard.sh`. Also commit the file as executable, as `scripts/check-public-surface.sh` is.

**Must-not**:
- Use `mv -T` or `mv --no-target-directory`. BSD `mv` on macOS does not have them.
- Delete or replace a directory found at a target path. The owner must see it.
- Change the reviewer steps, the usable-report check, the synthesis logic, or the `verdict_gate` steps.
- Move the existing stdout lines of the lok write steps (`REVIEW_INPUT_FAILED: see ...`, `NO_REVIEWS_AVAILABLE`, `SYNTHESIS_FAILED`) to stderr. That is a separate CLO-945 follow-up. List it in the PR body.
- Fix other CLO-945 follow-ups (CLO-949 and others).

**Prefer**:
- One function in the guard library over per-site checks.
- Document the function in the header comment of `review-guard.sh`, in the same format as the other functions.

**Escalate when**:
- A publish site needs a control-flow change beyond replacing the `mv -f` command.
- `lok run` with a failing `write_reviews` step exits 0, or does not show the failed step's stderr. Both contradict the probe in the Problem Statement.

## 4. Decomposition

1. **Guard helper**: add `review_publish` to `.lok/lib/review-guard.sh` and document it in the header. Files: `.lok/lib/review-guard.sh`.
2. **Test script and CI**: add `scripts/test-review-guard.sh`. It runs from the repository root, sources the guard, and works in a temporary directory that it removes on exit.
   - Behavior cases: the target is a directory, a symbolic link to a directory, missing, or a regular file, and `mv` fails because the target's parent directory is read-only. Skip the read-only case with a printed note when the script runs as root. For each case, check the return code, stdout, stderr, the target and the source.
   - Wiring check: `mv -f` occurs 0 times in the four gate files.
   - Wiring check: the `review_publish` call count per file is spec-review.toml 4, design-review.toml 4, pre-pr-validation.toml 6, implement.md 3.
   - Wiring check: every line that calls `review_publish` contains `|| exit 1` or `elif !`.
   - CI: add a step `Review guard test` that runs `sh scripts/test-review-guard.sh`, next to the surface check.
   Files: `scripts/test-review-guard.sh`, `.github/workflows/ci.yml`.
3. **lok workflows**: replace the 14 `mv -f` moves with `review_publish`. Use the diff-site form from Constraints at `pre-pr-validation.toml:49`. Files: `.lok/workflows/spec-review.toml`, `.lok/workflows/design-review.toml`, `.lok/workflows/pre-pr-validation.toml`.
4. **Inline gate**: replace the 3 `mv -f` moves (2 lines) in the block. Use the diff-site form at line 69. Add `REVIEW_PUBLISH_FAILED:` to the text that explains `gate exit 1`. Files: `.claude/commands/task/phases/implement.md`.
5. **Reader docs**: add the AC10 sentence to the four places. Files: `.claude/commands/task/phases/spec.md`, `.claude/commands/task/phases/design.md`, `.claude/commands/design-doc/review.md`, `.lok/workflows/pre-pr-validation.toml` (header comment).

**Dependency order**: sub-task 1 blocks sub-tasks 2, 3 and 4. Sub-tasks 2, 3, 4 and 5 are independent of each other. The wiring check in sub-task 2 passes only after sub-tasks 3 and 4 are done.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Helper, target is a directory | stderr `REVIEW_PUBLISH_FAILED: <t>: target is a directory`, empty stdout, non-zero; src still exists; the directory gets no new entry | `sh scripts/test-review-guard.sh` |
| 2 | Helper, target is a symbolic link to a directory | Same as #1 | `sh scripts/test-review-guard.sh` |
| 3 | Helper, target missing | Returns 0, no output; target is a regular file with the src content; src is gone | `sh scripts/test-review-guard.sh` |
| 4 | Helper, target is a regular file | Same as #3, and the old content is replaced | `sh scripts/test-review-guard.sh` |
| 5 | Helper, `mv` fails (the target's parent is read-only) | stderr `REVIEW_PUBLISH_FAILED: <t>: mv failed: ...` (one line), non-zero | `sh scripts/test-review-guard.sh` (skipped with a note as root) |
| 6 | Wiring | No `mv -f`; the call counts are 4/4/6/3; every call has failure handling | `sh scripts/test-review-guard.sh` |
| 7 | CI | `Review guard test` passes on ubuntu-latest and macos-latest | PR checks |
| 8 | spec-review, synthesis target is a directory | `lok run` output has `✗ REVIEW_PUBLISH_FAILED: docs/reviews/<slug>-spec-review-synthesis.md: target is a directory`; no `usable=` line; `lok run` exits non-zero | `mkdir` the target, stub reviewers on `PATH`, `lok run .lok/workflows/spec-review.toml ... --verbose; echo $?` |
| 9 | design-review, codex target is a directory | Same pattern as #8 for `docs/reviews/<slug>-review-codex.md` | Same method, `design-review.toml` |
| 10 | pre-pr-validation, diff target is a directory | First line of `docs/reviews/<slug>-validation-synthesis.md` is `REVIEW_INPUT_FAILED: cannot write .lok/local/review-input/<slug>-branch.diff: target is a directory`; no reviewer runs; `lok run` exits non-zero | `mkdir` the diff target, then `lok run .lok/workflows/pre-pr-validation.toml ...` |
| 11 | pre-pr-validation, report target is a directory | Same pattern as #8 | Same method |
| 12 | Inline gate, diff target is a directory | `REVIEW_INPUT_FAILED: cannot write ...: target is a directory`, `gate exit 3` | Run the `implement.md` block with stub `codex` and `opencode` on `PATH` |
| 13 | Inline gate, report target is a directory | `REVIEW_PUBLISH_FAILED: ...`; no `codex=`, `gemini=` or `usable=` lines; `gate exit 1` | Same method |
| 14 | Normal case for all four gates | Reports at the same paths and the same success lines as on `main` | Stub runs of #8-#13 with no directory at the targets |
| 15 | Reader docs | The four places from AC10 name `REVIEW_PUBLISH_FAILED` | Read the diff |

**Edge cases to verify**:
- A regular file at the target is replaced, not refused (#4).
- A symbolic link to a directory is refused (#2).
- A failed publish of a later report leaves the earlier reports of the same run published. The non-zero exit already marks the run as not trustworthy, and the reader docs read no report in that case. This spec adds no rollback.
- The stub runs use a scratch slug. Afterwards the evaluation removes the directories and reports that it created.
