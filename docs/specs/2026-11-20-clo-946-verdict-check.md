# Spec: Fix the review verdict check passing a report when a later verdict says FAIL

**Created**: 2026-11-20
**Linear**: [CLO-946](https://linear.app/cloud-ai/issue/CLO-946)
**Estimated scope**: S (6 files: 5 modified, 1 new; 4 sub-tasks)

## 1. Problem Statement

The task-phase review gates (CLO-945, PR #90) check every reviewer output with one shared function before they accept it as a review. The function is `review_usable <phase> <file>` in `.lok/lib/review-guard.sh:39-142`. It exits 0 when the file is a usable report for the phase (`review`, `prepr`, `synthesis-review` or `synthesis-prepr`), 1 when it is not, and 2 for an unknown phase. Four callers use the result:

- `review_guard` (`.lok/lib/review-guard.sh:188-190`) prints `REVIEW_FAILED: <reviewer> <model>: not a usable report` instead of the report. The inline pre-PR gate in `.claude/commands/task/phases/implement.md` uses `review_guard`.
- `review_classify` (`.lok/lib/review-guard.sh:196-206`) prints `usable` or a failure reason; the `write_one` helpers of the three lok workflows write that reason into the failure report.
- The synthesis branch of each lok workflow (`.lok/workflows/spec-review.toml:343-350`, `.lok/workflows/design-review.toml:384-391`, `.lok/workflows/pre-pr-validation.toml:423-430`) writes `SYNTHESIS_FAILED: synthesis output failed the synthesis check` when the synthesis is not usable.

`review_usable` decides only whether a report is usable. No shell step reads the verdict value; a usable FAIL report is a real review and the orchestrator reads its findings.

**The defect.** The verdict part of `review_usable` (the `END` block, `.lok/lib/review-guard.sh:99-125`) walks the headings that contain the word "verdict" and stops at the first one that yields exactly one verdict token (the heading remainder plus the next 3 non-empty lines). It never looks at later verdict headings. Two consequences, reported by Codex as HIGH in the PR #90 pre-PR review (`docs/reviews/clo-945-codex-validation.md:15`):

1. A report with `## Verdict: PASS` and a later `## Final Verdict: FAIL` is usable, with an unstated verdict. For the binding pre-PR synthesis this hides a FAIL.
2. A placeholder sentence such as `## Verdict: PASS if no finding is HIGH or above` is usable, because `single_verdict` only rejects an explicit alternation (`PASS | FAIL`, `PASS or FAIL`).

**Assumption (stated for the checkpoint).** The ticket says "if any verdict heading says FAIL, the gate does not pass". This spec reads "the gate does not pass" as "`review_usable` rejects the report" only when the verdicts disagree. A report whose only verdict is FAIL, once or repeated, stays usable with verdict FAIL. Rejecting every FAIL report would turn each real FAIL review into `REVIEW_FAILED: not a usable report` and drop its findings.

**Placeholder definition (owner decision, 2026-11-20).** A verdict line is a placeholder, and therefore not a verdict, when it has one of these template forms:

- (P1) an alternation of verdict tokens: `PASS | PASS_WITH_NOTES | FAIL`, `PASS or FAIL`, `PASS/FAIL` (the forms `single_verdict` already detects);
- (P2) a token in brackets or angle brackets: `[PASS | PASS_WITH_NOTES | FAIL]`, `<PASS>` (today these do not parse as a token at all, so they are already ignored);
- (P3) a token followed directly by a condition word: `if`, `when`, `unless`, `otherwise` (case-insensitive, after optional closing markdown).

Any other line that starts with a verdict token is a verdict, including a token followed by prose: `**PASS_WITH_NOTES** - two LOW findings` and `**APPROVE_WITH_SUGGESTIONS** (both reviewers agree; no NEEDS_REVISION)` stay verdicts with the first token as the value.

**Effect on stored reports.** A prototype of this change (scratch copy of the guard) gives the same result as the current guard for all 206 files in `docs/reviews/` in all four phases (824 checks, no difference). Three stored reports have more than one distinct verdict heading: `clo-491-codex-revalidation.md`, `clo-491-codex-validation.md` and `clo-537-codex-validation.md`. Each is a raw Codex transcript that echoes the prompt's template heading (P1 or P2), then gives the real verdict twice. The template line is ignored, the repeated verdict counts once, and each report stays usable. **The CLO-537 report therefore keeps its result: usable, verdict FAIL.** The ticket expected this report to change; it does not, because the ticket also rules that a template line is not a verdict.

## 2. Acceptance Criteria

- [ ] AC1: `review_usable` scans every heading that contains "verdict" in the whole report (outside code fences), not only the first one that yields a verdict. Each candidate line (the heading remainder and the next 3 non-empty lines before the next heading, as today) gives one of: no token, a placeholder (P1-P3), or one verdict value.
- [ ] AC2: Verdict values are compared by the canonical token: `PASS`, `PASS_WITH_NOTES`, `FAIL`, `APPROVE`, `APPROVE_WITH_SUGGESTIONS`, `NEEDS_REVISION`, `APPROVE_WITH_CHANGES`, `REWORK`, with the phase rules of today's `verdict_token` (the `prepr` phase compares upper-cased). `PASS` and `PASS_WITH_NOTES` are different verdicts.
- [ ] AC3: Zero distinct verdict values: exit 1 (as today). One distinct value, however often repeated: the verdict check passes and the heading checks run as today. Two or more distinct values: exit 3.
- [ ] AC4: A report with `## Verdict: PASS` and a later `## Final Verdict: FAIL` exits 3 in `prepr` and `synthesis-prepr`.
- [ ] AC5: A report whose only verdict line is a placeholder exits 1. Fixtures: one per form P1, P2, P3 (P3 with `if` and with `when`).
- [ ] AC6: A report with one PASS verdict repeated identically under two verdict headings exits 0 in `prepr`. The same holds for FAIL.
- [ ] AC7: A report with a placeholder heading followed by a real verdict heading exits 0. A real verdict with trailing prose (`**PASS_WITH_NOTES** - two LOW findings, no PASS blocker`) exits 0.
- [ ] AC8: The conflict is a named failure. `review_guard` prints `REVIEW_FAILED: <reviewer> <model>: conflicting verdicts` when `review_usable` exits 3. `review_classify` prints `conflicting verdicts` for exit 3. The synthesis branch in each of the three lok workflows writes `SYNTHESIS_FAILED: synthesis output has conflicting verdicts` when `review_usable` exits 3. Every other non-zero exit keeps its current message.
- [ ] AC9: The line whose position the `prepr` heading count excludes (`vline`) stays the first verdict heading that yields a value, so the `other >= 1` rule gives the same result as today.
- [ ] AC10: The new check gives the same result as the current guard for every file in `docs/reviews/` in all four phases, except files added by this task. The three AC19 reports of CLO-945 (`docs/reviews/clo-945-spec-review-gemini.md`, `docs/reviews/clo-945-spec-review-codex.md`, `docs/reviews/clo-942-codex-validation.md`) pass as before.
- [ ] AC11: `docs/reviews/clo-537-codex-validation.md` is run through the new check in phase `prepr`, and the PR body records the result (expected: exit 0, verdict FAIL, template line at line 30 ignored).
- [ ] AC12: The spec-review reports of this task (`docs/reviews/clo-946-spec-review-*.md`), which quote `## Verdict: PASS` and `## Final Verdict: FAIL` in their text, are not broken by the new check: each report that the run wrote as a review (not a `REVIEW_FAILED:` report) exits 0 in phase `review`.
- [ ] AC13: A committed test script runs every fixture of AC4-AC8 and the AC10/AC11 checks, exits non-zero on any mismatch, and runs in CI on `ubuntu-latest` and `macos-latest`.

**Verification method**: AC1-AC9 and AC13 by the test script (Evaluation #1-#9) on macOS awk locally and in the CI matrix. AC4 and AC5 fixtures are also run against the unmodified guard to show they exit 0 there (red first). AC10 by the before/after sweep (Evaluation #10). AC11 by a direct call (Evaluation #11). AC12 after the lok spec-review run (Evaluation #12).

## 3. Constraints

**Must**:
- Keep `review-guard.sh` POSIX `sh` plus portable awk: it must run on macOS one-true-awk (version 20200816) and on the awk of `ubuntu-latest` (mawk or gawk). No gawk-only functions (`gensub`, `asort`, `IGNORECASE`, `\y`).
- Reuse `verdict_token` and `single_verdict` for token detection and the P1 test, so the accepted token set does not change.
- Keep exit 2 for an unknown phase, and keep every caller's handling of exit 1.
- Ignore a placeholder line. A placeholder never counts as a disagreeing verdict.
- Keep the test fixtures self-contained (written by the test script into a temporary directory and removed afterwards); do not add fixture files under `docs/reviews/`.

**Must-not**:
- Must not reject a report because its only verdict is FAIL.
- Must not treat trailing prose after a verdict token as a placeholder, except the P3 condition words.
- Must not change: code fence tracking, the `NO_REVIEWS_AVAILABLE` handling, the `review`/`prepr` heading-count rules, `REVIEW_GUARD_SKIP_USABLE`, the length and help-text checks in `review_guard`. These are CLO-945 follow-ups tracked elsewhere (CLO-949).
- Must not edit the review prompt templates.

**Prefer**:
- One new awk function that returns the canonical value, `""` for no token, or a placeholder marker; the `END` loop collects distinct values in an array.
- Test script at `.lok/tests/review-guard-test.sh`, run as `sh .lok/tests/review-guard-test.sh` from the repository root, printing one `ok`/`FAIL` line per case.

**Escalate when**:
- The before/after sweep (AC10) shows any difference on a stored report.
- Ubuntu CI awk gives a different result from macOS awk on any fixture.
- A lok spec-review report of this task becomes unusable under the new check (AC12).

## 4. Decomposition

1. **Verdict scan**: add the value function (canonical token, placeholder P1-P3) and replace the first-match loop in the `END` block with the whole-report collection and exit 3 for a conflict; keep `vline` as the first heading that yields a value. Update the `review_usable` line in the header comment (exit 3). - files: `.lok/lib/review-guard.sh`
2. **Named failure**: map exit 3 to `conflicting verdicts` in `review_guard` and `review_classify`; in each workflow's synthesis branch, capture the `review_usable` exit status and set `WHY="synthesis output has conflicting verdicts"` for 3. - files: `.lok/lib/review-guard.sh`, `.lok/workflows/spec-review.toml`, `.lok/workflows/design-review.toml`, `.lok/workflows/pre-pr-validation.toml`
3. **Tests**: write the test script with the fixtures of Evaluation #1-#9 and the AC10 checks on the three AC19 reports and the CLO-537 report; add one CI step after the surface check. - files: `.lok/tests/review-guard-test.sh` (new), `.github/workflows/ci.yml`
4. **Evidence**: run the before/after sweep against the guard at `main` (AC10), run the CLO-537 check (AC11), and record both results in the PR body. - files: none (PR body)

**Dependency order**: 2 depends on 1. 3 can be written in parallel with 1 and must pass after 1 and 2. 4 runs last.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `## Verdict: PASS` ... `## Final Verdict: FAIL` (+ Findings, Missing Items headings) | exit 3 in `prepr`; exit 0 on the guard at `main` | test script; red check against `git show main:.lok/lib/review-guard.sh` |
| 2 | `## Verdict: PASS_WITH_NOTES` ... `## Verdict` / `PASS` | exit 3 in `prepr` | test script |
| 3 | Pre-PR synthesis: `## Verdict` / `**PASS**`, `## Must Fix Before PR`, `## Final Verdict` / `FAIL` | exit 3 in `synthesis-prepr`; exit 0 on `main` | test script |
| 4 | Only verdict `## Verdict: PASS if no finding is HIGH or above` (P3 `if`); `## Verdict` / `PASS when every criterion is met` (P3 `when`) | exit 1; both exit 0 on `main` | test script |
| 5 | Only verdict `## Verdict: PASS \| PASS_WITH_NOTES \| FAIL` (P1); `## Verdict: [PASS \| PASS_WITH_NOTES \| FAIL]` (P2) | exit 1 | test script |
| 6 | `## Verdict: PASS` twice; `## Verdict: FAIL` twice | exit 0 in `prepr` | test script |
| 7 | P1 template heading, then `## Verdict: FAIL` | exit 0 in `prepr` | test script |
| 8 | `## Verdict` / `**PASS_WITH_NOTES** - two LOW findings, no PASS blocker` | exit 0 in `prepr` | test script |
| 9 | Named reasons: `review_guard` on fixture #1 prints `REVIEW_FAILED: Codex m: conflicting verdicts`; `review_classify true <#1> prepr` prints `conflicting verdicts`; on a no-verdict report both keep `not a usable report` | as stated | test script |
| 10 | Before/after sweep: `review_usable` in all four phases on every `docs/reviews/*.md`, guard at `main` vs branch | no difference, except files added by this task | scratch sweep script; list result in PR |
| 11 | CLO-537 report, phase `prepr` | exit 0 (template line 30 ignored, two identical FAIL verdicts) | `. .lok/lib/review-guard.sh; review_usable prepr docs/reviews/clo-537-codex-validation.md; echo $?` |
| 12 | This task's spec-review reports | each review-form report exits 0 in `review` | after `lok run .lok/workflows/spec-review.toml ...` |
| 13 | Synthesis branch message | a synthesis stub with conflicting verdicts gives `SYNTHESIS_FAILED: synthesis output has conflicting verdicts` | read the three changed branches; run the branch logic from `pre-pr-validation.toml` on fixture #3 in a scratch shell |
| 14 | CI | the new step passes on `ubuntu-latest` and `macos-latest` | PR checks |

**Edge cases to verify**:
- A verdict token inside a code fence does not count (fence handling unchanged).
- Lower-case `pass` in `prepr` is not a token (unchanged phase rules).
- Two verdict tokens inside one section (heading `## Verdict`, then `**PASS**`, then a line starting with `PASS_WITH_NOTES`) are a conflict.
- A heading such as `## Verdict rationale` with prose lines that do not start with a token adds no value.
- `review` phase: `APPROVE` and `APPROVE_WITH_SUGGESTIONS` under two headings are a conflict.
