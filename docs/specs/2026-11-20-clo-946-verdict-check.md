# Spec: Fix the review verdict check passing a report when a later verdict says FAIL

**Created**: 2026-11-20
**Linear**: [CLO-946](https://linear.app/cloud-ai/issue/CLO-946)
**Estimated scope**: S (6 files: 5 modified, 1 new; 4 sub-tasks)

## 1. Problem Statement

The task-phase review gates (CLO-945, PR #90) check every reviewer output with one shared function before they accept it as a review. The function is `review_usable <phase> <file>` in `.lok/lib/review-guard.sh:39-142`. It exits 0 when the file is a usable report for the phase (`review`, `prepr`, `synthesis-review` or `synthesis-prepr`), 1 when it is not, and 2 for an unknown phase. Four callers use the result:

- `review_guard` (`.lok/lib/review-guard.sh:188-190`) prints `REVIEW_FAILED: <reviewer> <model>: not a usable report` instead of the report. The inline pre-PR gate in `.claude/commands/task/phases/implement.md` uses `review_guard`.
- `review_classify` (`.lok/lib/review-guard.sh:196-206`) prints `usable` or a failure reason; the `write_one` helpers of the three lok workflows write that reason into the failure report.
- The synthesis branch of each lok workflow (`.lok/workflows/spec-review.toml:343-350`, `.lok/workflows/design-review.toml:384-391`, `.lok/workflows/pre-pr-validation.toml:423-430`) writes `SYNTHESIS_FAILED: synthesis output failed the synthesis check` when the synthesis is not usable. The three `case` blocks are identical.

`review_usable` decides only whether a report is usable. No shell step reads the verdict value. A usable FAIL report is a real review: in Pi, the binding pre-PR synthesis verdict FAIL stops the PR (`.pi/orchestrator/phases/implement.md`); in the inline Claude gate, the owner reads the report and decides (`.claude/commands/task/phases/implement.md`).

**How the verdict check works today.** The verdict part of `review_usable` (the `END` block, `.lok/lib/review-guard.sh:99-125`) walks the headings (`#`, `##`, `###` ...) that contain the word "verdict", outside code fences. For each such heading it reads one *section*: the heading text after "verdict", plus the next 3 non-empty lines before the next heading. Each line is tested with `verdict_token` (does the line start with a verdict token for the phase) and `single_verdict` (is the token followed by an alternation such as `| FAIL`). A section is *good* when exactly one of its lines starts with a token and that line is not an alternation; a section with no token line, two or more token lines, or an alternation is ignored. The loop stops at the first good section and records its heading line as `vline`. The `prepr` heading count later excludes `vline`.

**The defect.** Two cases pass that must not, reported by Codex as HIGH in the PR #90 pre-PR review (`docs/reviews/clo-945-codex-validation.md:15`):

1. A report with `## Verdict: PASS` and a later `## Final Verdict: FAIL` is usable, because the loop never looks past the first good section. For the binding pre-PR synthesis this hides a FAIL.
2. A placeholder sentence such as `## Verdict: PASS if no finding is HIGH or above` is usable, because `single_verdict` only rejects an alternation.

A third case comes from the alternation test itself. `single_verdict` treats a dash followed by any token as an alternation, so `## Final Verdict: FAIL - PASS criteria were not met` is not a verdict. Once the check reads later sections, a real FAIL written this way would be ignored, and a report with an earlier `## Verdict: PASS` would still pass.

**Assumption (stated for the checkpoint).** The ticket says "if any verdict heading says FAIL, the gate does not pass". This spec reads "the gate does not pass" as "`review_usable` rejects the report" only when the verdicts disagree. A report whose only verdict is FAIL, once or repeated, stays usable with verdict FAIL, and the existing Pi and inline gates act on it as described above. Rejecting every FAIL report would turn each real FAIL review into `REVIEW_FAILED: not a usable report` and drop its findings.

**Placeholder definition (owner decision, 2026-11-20).** A verdict line is a placeholder, and therefore not a verdict, when it has one of these template forms. "Token" means a verdict token of the phase (AC2). "Closing markdown" means any run of `*`, `_`, `` ` `` and horizontal whitespace.

- (P1) An alternation of tokens. After the first token and optional closing markdown comes a separator, optional closing markdown, and a second token that ends at a non-word character or at the end of the line. Separators: `/`, `|`, `,`, `&`, the words `or` and `and` followed by whitespace (any case), and the dashes `-`, `--`, em dash, en dash. For a dash separator only, the second token must also end the line (followed by nothing but closing markdown, `.` or `)`) or be followed by another separator. Examples that are placeholders: `PASS | PASS_WITH_NOTES | FAIL`, `PASS or FAIL depending on the findings`, `PASS/FAIL`, `PASS - FAIL`. Examples that are verdicts: `FAIL - PASS criteria were not met` (value FAIL), `PASS_WITH_NOTES - two LOW findings, no PASS blocker` (value PASS_WITH_NOTES). The dash rule narrows today's `single_verdict`, which treats `FAIL - PASS criteria were not met` as an alternation.
- (P2) A token inside square or angle brackets: `[PASS | PASS_WITH_NOTES | FAIL]`, `<PASS>`. `verdict_token` already finds no token in such a line, so the line is ignored today and stays ignored.
- (P3) A token followed by closing markdown that contains at least one space or tab, then one of the words `if`, `when`, `unless`, `otherwise` (any case), then a non-word character or the end of the line. Placeholders: `PASS if no finding is HIGH or above`, `**FAIL** otherwise`. Verdicts: `PASS iffy wording`, `PASS whenever` (no word boundary).
- (P4) Phase `synthesis-prepr` only: a line that, after trimming leading and trailing whitespace, is exactly one of the three legend bullets of the pre-PR synthesis prompt (`.lok/workflows/pre-pr-validation.toml:314-316`): `- PASS - no Must Fix items`, `- PASS_WITH_NOTES - Must Fix items exist but can be addressed in ONE bounded fix iteration`, `- FAIL - any Pivot/Fundamental Scope Issue is present, OR the change diverges materially from the design, OR the change cannot be salvaged in one fix iteration`. These three literal lines are reserved: a synthesis that copies one of them as its conclusion gives no value. Any other wording stays a verdict, for example `PASS - no Must Fix items in this diff` or `**PASS** - no Must Fix items`. In the other phases these lines are ordinary verdicts.

P2 differs from P1, P3 and P4 in the per-section rule. A P2 line does not start with a token, so it is not a token line: it neither gives a value nor invalidates its section, and a real verdict on the next line of the same section still counts (as today). A P1, P3 or P4 line starts with a token, so it is a token line that is a placeholder, and its section gives no value.

Any other line that starts with a token is a verdict whose value is that first token, also when prose follows it.

**Prompt legend.** Before this change, a synthesis that copies one legend bullet under `## Verdict` is usable with that bullet's token as its verdict. P4 closes that gap. An echo of the whole legend was already ignored, because its section has more than one token line, and it stays ignored.

**Effect on stored reports.** A prototype of this change (scratch copy of the guard) gives the same result as the guard at `main` (`9772049`) for all 209 files in `docs/reviews/` in all four phases (836 checks, no difference). Three stored reports have more than one verdict heading with a value: `clo-491-codex-revalidation.md`, `clo-491-codex-validation.md` and `clo-537-codex-validation.md`. Each is a raw Codex transcript that echoes the prompt's template heading (P1), then gives the real verdict twice. The template line is ignored, the repeated verdict counts once, and each report stays usable. **The CLO-537 report therefore keeps its result: usable, verdict FAIL.** The ticket expected this report to change; it does not, because the ticket also rules that a template line is not a verdict.

## 2. Acceptance Criteria

- [ ] AC1: `review_usable` reads every section (as defined in the Problem Statement: heading text plus at most the next 3 non-empty lines before the next heading, outside code fences) of every heading that contains "verdict", not only up to the first good one. The 3-line limit stays: a token more than 3 non-empty lines below a verdict heading is not read, as today.
- [ ] AC2: Tokens and their case rules are those of today's `verdict_token`. Phases `review` and `synthesis-review`: `APPROVE_WITH_SUGGESTIONS`, `NEEDS_REVISION`, `APPROVE`, upper case only. Phase `synthesis-prepr`: `PASS_WITH_NOTES`, `PASS`, `FAIL`, upper case only. Phase `prepr`: `PASS_WITH_NOTES`, `PASS`, `FAIL` upper case only, plus `APPROVE_WITH_CHANGES`, `APPROVE`, `REWORK` in any case (compared upper-cased). A line that `verdict_token` rejects never gives a value. The value of a line is its first token; values are compared as strings, so `PASS` and `PASS_WITH_NOTES` differ.
- [ ] AC3: The per-section rule is unchanged: a section gives a value only when exactly one of its lines starts with a token and that line is not a P1, P3 or P4 placeholder. A section with a P1, P3 or P4 line, or with two or more token lines, gives no value. A P2 line is not a token line: it does not count toward the token lines of its section and does not invalidate the section, so `## Verdict: [PASS | PASS_WITH_NOTES | FAIL]` followed by the body line `FAIL` gives the value FAIL.
- [ ] AC4: Zero distinct values over all sections: exit 1 (as today). Exactly one distinct value, given by one section or repeated in several: the verdict check passes, and the heading checks run as today. Two or more distinct values: exit 3.
- [ ] AC5: `vline` is the heading line of the first section that gives a value. When the first section that gives a value is the same section that today's loop selects, `vline` and the `prepr` heading count are unchanged. Example: `## Verdict: PASS`, a body line `PASS`, `### Final Verdict: PASS` gives the same exit status as on `main`. When a P3 (or P4) line made today's first good section, that section now gives no value and the next one becomes `vline`. Expected change: in `prepr`, `### Verdict: PASS if checks pass` followed by `## Final Verdict: PASS` and no other `##` heading exits 0 on `main` (the H3 is `vline`, the H2 counts as another heading) and exits 1 on the branch (the H2 is `vline`, no other H2 remains). The corpus sweep (AC10) shows no stored report of this shape.
- [ ] AC6: Conflicts exit 3: `## Verdict: PASS` ... `## Final Verdict: FAIL` in `prepr` and `synthesis-prepr`; the same with `## Final Verdict: FAIL - PASS criteria were not met`; `PASS_WITH_NOTES` against `PASS`; `APPROVE` against `APPROVE_WITH_SUGGESTIONS` in `review`; `APPROVE` against `NEEDS_REVISION` in `synthesis-review`; a P1 template heading followed by `PASS` and a later `FAIL`; in `prepr`, `## Verdict: PASS` followed later by `## Verdict: [PASS | PASS_WITH_NOTES | FAIL]` with the body line `FAIL` (P2 contributes no token line, so the FAIL counts).
- [ ] AC7: A report whose only verdict lines are placeholders exits 1: one fixture per form P1 (`|`, `or` with trailing prose, dash), P2, P3 (`if`, `when`, `unless`, `otherwise`), and P4: in `synthesis-prepr`, `## Verdict`, one copied legend bullet, and `## Must Fix Before PR` exits 1, one fixture per bullet.
- [ ] AC8: These exit 0 in `prepr`: one PASS repeated identically under two verdict headings; one FAIL repeated the same way; a P1 template heading followed by a real `## Verdict: FAIL`; a real verdict with trailing prose (`**PASS_WITH_NOTES** - two LOW findings, no PASS blocker`); `PASS iffy wording`; `## Verdict: [PASS | PASS_WITH_NOTES | FAIL]` with the body line `FAIL` and a later `##` heading. In `synthesis-prepr`, `PASS - no Must Fix items in this diff` and `**PASS** - no Must Fix items` (not exact legend lines) under `## Verdict` with `## Must Fix Before PR` exit 0. In `synthesis-prepr`, a section that echoes the whole prompt legend followed by a real `## Verdict` / `FAIL` and a `## Must Fix Before PR` heading exits 0, and the legend echo alone (no other verdict) exits 1.
- [ ] AC9: The conflict is a named failure. `review_guard` prints `REVIEW_FAILED: <reviewer> <model>: conflicting verdicts` when `review_usable` exits 3. `review_classify` prints `conflicting verdicts` for exit 3. A new helper `review_synthesis_reason <syn-success> <phase> <file>` in `.lok/lib/review-guard.sh` prints the synthesis failure reason: `synthesis step did not run` (skipped), `synthesis output has conflicting verdicts` (true, `review_usable` exits 3), `synthesis output failed the synthesis check` (true, any other non-zero exit), `synthesis step failed` (anything else). The `case` block in each of the three workflows is replaced by one call to this helper. Every other non-zero exit keeps its current message. The header comments list exit 3, `conflicting verdicts` and the new helper.
- [ ] AC10: The guard on the branch gives the same result as the guard at `main` (`9772049`) for every file in `docs/reviews/` in all four phases, except files added by this task. The three AC19 reports of CLO-945 (`docs/reviews/clo-945-spec-review-gemini.md`, `docs/reviews/clo-945-spec-review-codex.md`, `docs/reviews/clo-942-codex-validation.md`) pass as before.
- [ ] AC11: `docs/reviews/clo-537-codex-validation.md` is run through the new check in phase `prepr`, and the PR body records the result (expected: exit 0, verdict FAIL, template line at line 30 ignored, two identical FAIL verdicts at lines 10418 and 10440).
- [ ] AC12: The spec-review reports of this task stay usable under the new check: `docs/reviews/clo-946-spec-review-gemini.md` and `...-codex.md` exit 0 in phase `review`, and `docs/reviews/clo-946-spec-review-synthesis.md` exits 0 in phase `synthesis-review`.
- [ ] AC13: A committed POSIX `sh` test script (no bashisms; it runs under `dash`) runs every fixture of AC5-AC9 and the AC10 checks on the three AC19 reports and the CLO-537 report, exits non-zero on any mismatch, and runs in CI on `ubuntu-latest` and `macos-latest`. CI runs only this script; the workflow stub runs (Evaluation #13) and the full corpus sweep (Evaluation #10) are PR evidence, because CI has no `lok`.

**Verification method**: AC1-AC9 and AC13 by the test script (Evaluation #1-#9) on macOS awk locally and in the CI matrix. The conflict, P2-then-body, P3 and P4 fixtures are also run against the guard at `main` to show they exit 0 there (red first). AC9 wiring in the three workflows by stub runs (Evaluation #13). AC10 by the before/after sweep (Evaluation #10). AC11 by a direct call (Evaluation #11). AC12 by Evaluation #12.

## 3. Constraints

**Must**:
- Keep `review-guard.sh` POSIX `sh` plus portable awk: it must run on macOS one-true-awk (version 20200816) and on the awk of `ubuntu-latest` (mawk or gawk). No gawk-only features (`gensub`, `asort`, `IGNORECASE`, `\y`, `length(array)`); count distinct values with a scalar counter.
- Reuse `verdict_token` for token detection, and test it before any upper-casing, so the accepted token set and its case rules do not change.
- Keep exit 2 for an unknown phase, and keep every caller's handling of exit 1.
- Ignore a placeholder line. A placeholder never counts as a disagreeing verdict.
- In each workflow, call the reason helper only in the failure branch; the success condition (`[ "$SYN_SUCCESS" = true ] && review_usable ...`) stays as it is.
- Keep the test fixtures self-contained (written by the test script into a temporary directory and removed afterwards). Pad every fixture that goes through `review_guard` above its 100-character minimum, so the test reaches the verdict check.

**Must-not**:
- Must not reject a report because its only verdict is FAIL.
- Must not treat trailing prose after a verdict token as a placeholder, except P1, P3 and P4.
- Must not change: the per-section rule, the 3-line lookahead, code fence tracking, the `NO_REVIEWS_AVAILABLE` handling, the `review`/`prepr` heading-count rules, `REVIEW_GUARD_SKIP_USABLE`, the length and help-text checks in `review_guard`. These are CLO-945 follow-ups tracked elsewhere (CLO-949).
- Must not edit the review prompt templates.

**Prefer**:
- One awk function that returns the value, `""` for no token, or a placeholder marker, replacing `single_verdict`.
- Test script at `.lok/tests/review-guard-test.sh`, run as `sh .lok/tests/review-guard-test.sh` from the repository root, printing one `ok`/`FAIL` line per case.

**Escalate when**:
- The before/after sweep (AC10) shows any difference on a stored report.
- Ubuntu CI awk gives a different result from macOS awk on any fixture.
- A report of this task's own lok runs becomes unusable under the new check. The CLO-946 pre-PR run uses phases `prepr` and `synthesis-prepr`, where `PASS` and `FAIL` are tokens; a reviewer who writes a fixture heading such as `## Final Verdict: FAIL` at column 0 outside a code fence produces a report that exits 3. Report that to the owner as a named failure and do not change the guard for it.

## 4. Decomposition

1. **Verdict scan**: replace `single_verdict` with the value function (token, P1 with the dash rule, P3, P4 in `synthesis-prepr`); replace the first-match loop in the `END` block with the all-section collection, keep the per-section rule, set `vline` to the first section with a value, and exit 3 on two distinct values. Update the header comment (exit 3). - files: `.lok/lib/review-guard.sh`
2. **Named failure**: map exit 3 to `conflicting verdicts` in `review_guard` and `review_classify`; add `review_synthesis_reason`; replace the `case` block in each workflow's synthesis failure branch with `WHY=$(review_synthesis_reason "$SYN_SUCCESS" <phase> "$RUN/synthesis.final")`; update the header comments of `review_classify` and the new helper. - files: `.lok/lib/review-guard.sh`, `.lok/workflows/spec-review.toml`, `.lok/workflows/design-review.toml`, `.lok/workflows/pre-pr-validation.toml`
3. **Tests**: write the test script with the fixtures of Evaluation #1-#9 and the AC10 checks on the three AC19 reports and the CLO-537 report; add one CI step after the surface check. - files: `.lok/tests/review-guard-test.sh` (new), `.github/workflows/ci.yml`
4. **Evidence**: run the red-first fixtures against `main`; run the before/after sweep against `9772049` (AC10); run the CLO-537 check (AC11) and the AC12 check; run each of the three workflows once with stub CLIs whose synthesis has conflicting verdicts (Evaluation #13). Record the results in the PR body. - files: none (PR body)

**Dependency order**: 2 depends on 1. 3 can be written in parallel with 1 and must pass after 1 and 2. 4 runs last.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Conflicts: `## Verdict: PASS` ... `## Final Verdict: FAIL` (`prepr`, `synthesis-prepr`); `... FAIL - PASS criteria were not met`; `PASS_WITH_NOTES` vs `PASS`; P1 template, then `PASS`, then `FAIL` | exit 3; each exits 0 on `main` (red first) | test script; red check with `git show 9772049:.lok/lib/review-guard.sh` |
| 2 | Conflicts in the other phases: `APPROVE` vs `APPROVE_WITH_SUGGESTIONS` (`review`, with `## Actionable Feedback`); `APPROVE` vs `NEEDS_REVISION` (`synthesis-review`, with `## Priority Actions`) | exit 3; exit 0 on `main` | test script |
| 3 | Pre-PR synthesis: `## Verdict` / `**PASS**`, `## Must Fix Before PR`, `## Final Verdict` / `FAIL` | exit 3 in `synthesis-prepr`; exit 0 on `main` | test script |
| 4 | P3 only: `if`, `when`, `unless`, `otherwise` (one fixture each, `otherwise` as `**FAIL** otherwise`) | exit 1; each exits 0 on `main` | test script |
| 5 | P1 and P2 only: `PASS \| PASS_WITH_NOTES \| FAIL`; `PASS or FAIL depending on the findings`; `PASS - FAIL`; `[PASS \| PASS_WITH_NOTES \| FAIL]` | exit 1 (also 1 on `main`) | test script |
| 5a | P4 only (`synthesis-prepr`): `## Verdict`, one copied legend bullet, `## Must Fix Before PR`; one fixture per bullet | exit 1; each exits 0 on `main` | test script |
| 5b | P2 then body: `## Verdict: PASS`, later `## Verdict: [PASS \| PASS_WITH_NOTES \| FAIL]` with body `FAIL` (`prepr`) | exit 3; exit 0 on `main` | test script |
| 5c | H3 conditional then H2 real (`prepr`): `### Verdict: PASS if checks pass`, `## Final Verdict: PASS`, no other `##` heading | exit 1; exit 0 on `main` (documented change, AC5) | test script |
| 6 | Repeats: `## Verdict: PASS` twice; `## Verdict: FAIL` twice | exit 0 in `prepr` | test script |
| 7 | Not placeholders: P1 template heading then `## Verdict: FAIL`; `**PASS_WITH_NOTES** - two LOW findings, no PASS blocker`; `PASS iffy wording`; `## Verdict: [PASS \| PASS_WITH_NOTES \| FAIL]` with body `FAIL`; `## Verdict: PASS`, body `PASS`, `### Final Verdict: PASS` (same exit as `main`) | exit 0 in `prepr` | test script |
| 8 | Prompt legend (`synthesis-prepr`): whole legend echo then `## Verdict` / `FAIL` and `## Must Fix Before PR`; whole legend echo alone with `## Must Fix Before PR`; `PASS - no Must Fix items in this diff`; `**PASS** - no Must Fix items` | exit 0; exit 1; exit 0; exit 0 | test script |
| 9 | Named reasons: `review_guard` on the padded fixture #1 prints `REVIEW_FAILED: Codex m: conflicting verdicts`; `review_classify true <#1> prepr` prints `conflicting verdicts`; `review_synthesis_reason true synthesis-prepr <#3>` prints `synthesis output has conflicting verdicts`; on a padded no-verdict report they print `not a usable report` and `synthesis output failed the synthesis check`; `skipped` and `false` give their current texts | as stated | test script |
| 10 | Before/after sweep: `review_usable` in all four phases on every `docs/reviews/*.md`, guard at `9772049` vs branch | no difference, except files added by this task | scratch sweep script; result in PR body |
| 11 | CLO-537 report, phase `prepr` | exit 0 | `. .lok/lib/review-guard.sh; review_usable prepr docs/reviews/clo-537-codex-validation.md; echo $?` |
| 12 | This task's spec-review reports | Gemini and Codex exit 0 in `review`; synthesis exits 0 in `synthesis-review` | direct calls as in #11 |
| 13 | Synthesis wiring, all three workflows | with stub reviewer CLIs and a `claude` stub whose synthesis has conflicting verdicts in the phase's own tokens: for `spec-review` and `design-review` (phase `synthesis-review`), `## Verdict` / `APPROVE` and a later `## Final Verdict` / `NEEDS_REVISION`, plus `## Priority Actions`; for `pre-pr-validation` (phase `synthesis-prepr`), `## Verdict` / `PASS` and a later `## Final Verdict` / `FAIL`, plus `## Must Fix Before PR`. In each run the synthesis file's first line is `SYNTHESIS_FAILED: synthesis output has conflicting verdicts`; `pre-pr-validation` exits non-zero; `spec-review` and `design-review` exit 0 and still publish the usable individual reports | stub runs in the CLO-945 style (stubs first on `PATH`, test slug, test files deleted afterwards); result in PR body |
| 14 | CI | the new step passes on `ubuntu-latest` and `macos-latest` | PR checks |

**Edge cases to verify**:
- A verdict token inside a code fence does not count (fence handling unchanged).
- Lower-case `pass` in `prepr` is not a token; lower-case `approve` in `prepr` is (unchanged phase rules).
- A section with two token lines (heading `## Verdict`, then `**PASS**`, then `PASS_WITH_NOTES`) gives no value; it is not a conflict.
- A heading such as `## Verdict rationale` with prose lines that do not start with a token gives no value.
