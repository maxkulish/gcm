# Spec Review Synthesis: clo-946

**Reviewers**: Gemini (google-vertex/gemini-4-hydrogen#high), Codex (gpt-6.2-sol, effort high)
**Synthesized**: 2026-11-20
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| A1 | AC2 does not state the `prepr` case rule correctly. `verdict_token` (`.lok/lib/review-guard.sh:64-72`) folds case only for the older tokens `APPROVE_WITH_CHANGES`, `APPROVE` and `REWORK`. It matches `PASS`, `PASS_WITH_NOTES` and `FAIL` in upper case only. AC2 conflicts with Edge Case 2. Gemini adds that `single_verdict` calls `toupper` without a condition in `prepr` (line 84), so the new scan must check `verdict_token` first, or lower-case `pass` gets through. | High |
| A2 | The changes to the three workflow synthesis branches (`spec-review.toml`, `design-review.toml`, `pre-pr-validation.toml`) are only checked by hand in Evaluation #13, and only for `pre-pr-validation.toml`. No CI step covers them. The tests must run each real workflow writer with stubs. They must check the published first line (`SYNTHESIS_FAILED: synthesis output has conflicting verdicts`) and the outer exit status. | Medium |
| A3 | AC13, sub-task 3 and the Evaluation table do not agree on what runs in CI. AC13 puts AC4-AC8 (Gemini) and the AC10 corpus sweep (Codex) in the committed script. Evaluation #10 and #13 leave them to scratch scripts or manual runs. The spec must say which checks are in CI and which are PR evidence only. It must also give AC12 and Evaluation #13 to a sub-task. | Medium |
| A4 | The placeholder grammar is not precise enough to implement without guessing. Gemini: P3 does not define a word boundary after the condition word (`PASS iffy`, `PASS whenever`), and it does not list which closing markdown characters are stripped. Codex: "alternation" must be defined by the full token list, not by the current loose `single_verdict` regex (see D1). | Medium |
| A5 | Some fixtures are missing. Gemini: P3 `unless` and `otherwise`; a `synthesis-review` conflict; a `review`-phase conflict (`APPROVE` against `APPROVE_WITH_SUGGESTIONS`); a placeholder followed by two conflicting real verdicts (exit 3). Codex: a later `FAIL` with explanatory prose; a synthesis that only echoes the prompt. Both reviewers want the red-first checks run against `main`. | Medium |
| A6 | Exit code `3` fits the helper contract, and so do the `REVIEW_FAILED:` / `SYNTHESIS_FAILED:` line-1 prefixes. The callers in `.claude/commands/task/phases/*.md` and `pre-pr-validation.toml:440-443` keep working. The Rust boundary is not affected: the `Backend`, `BackendErrorKind` and `anyhow` criteria do not apply. | Info |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Codex Position | Claude Position |
|---|-------|-----------------|----------------|-----------------|
| D1 | Reusing `single_verdict` without changes | Aligned. Reuse keeps the token set and P1 detection. | High concern. A probe showed that `single_verdict` treats `FAIL - PASS criteria were not met` as an alternation. With reuse, `## Final Verdict: FAIL - PASS criteria were not met` after a valid `PASS` would be ignored, and the binding synthesis would stay usable. This also conflicts with AC7 (trailing prose is valid). | I side with Codex. Its claim comes from a probe it ran. Gemini only accepted the constraint as written. The spec must define an alternation as `TOKEN <sep> TOKEN [<sep> TOKEN...]` over the full phase token list, with nothing else on the line. It must also add the "later FAIL with prose" fixture with an expected exit 3. |
| D2 | AC9 keeps `vline` / heading-count behavior | Strong. AC9 keeps the `vline` semantics. | Gap. An in-memory probe showed a case where the count changes from 0 to 1 under the new selection: `## Verdict: PASS`, then a body line `PASS`, then `### Final Verdict: PASS`. | I side with Codex, because the counterexample is concrete. The spec must decide which heading sets `vline` when the same verdict repeats across heading levels. AC9 must then either match the current result or state the change. |
| D3 | Overall verdict | APPROVE_WITH_SUGGESTIONS | NEEDS_REVISION | NEEDS_REVISION. D1 is a correctness hole in the exact behavior the ticket is meant to fix. The rule also forces this result. |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| N1 | The pre-PR synthesis prompt has `- PASS - no Must Fix items` under `## Verdict` (`pre-pr-validation.toml:312`). Suppose a fragment echoes that line together with the required `Must Fix Before PR` heading. It passes the current check and is not caught by P1-P3. The spec must handle this prompt text explicitly. | Codex | Medium |
| N2 | The scan only looks three lines ahead, and the spec does not state this limit. AC1 promises a whole-report guarantee, so the spec must state the limit or remove it. | Codex | Medium |
| N3 | The AC12 glob also matches the CLO-946 synthesis file. That file runs in phase `synthesis-review` and needs `Priority Actions`, so it needs a different check from the reviewer reports. | Codex | Medium |
| N4 | The heading-only conflict fixture in Evaluation #1 is 68 characters. `review_guard` has a 100-character minimum (Evaluation #9), so the fixture would fail before verdict parsing and pass for the wrong reason. The wrapper fixtures need padding. | Codex | Medium |
| N5 | The red-first check must include the conflict and P3 cases. The P1 and P2 fixtures already return 1 on the current guard, so they do not prove the fix. | Codex | Medium |
| N6 | The pre-PR run for CLO-946 uses `prepr` / `synthesis-prepr`, where `PASS` and `FAIL` are tokens. If a reviewer quotes fixture headings at column 0 outside a code fence, the new guard rejects that report as conflicting. | Gemini | Medium |
| N7 | The spec must say what "the gate does not pass" means for a usable `FAIL` report. In Pi, the binding synthesis verdict blocks the PR. In the inline Claude gate, the owner sees the report and can choose to proceed. | Codex | Low |
| N8 | POSIX does not define `length(array)` in awk. Count distinct verdicts with a scalar counter so the code is safe on one-true-awk and mawk. | Gemini | Low |
| N9 | Sub-task 2 must also update the header comment of `review_classify` (`.lok/lib/review-guard.sh:16-18`) to list `conflicting verdicts`. | Gemini | Low |
| N10 | `.lok/tests/review-guard-test.sh` runs under `dash` on Ubuntu. It must not use bashisms (`local`, `[[ ]]`, arrays). | Gemini | Low |
| N11 | Fix the comparison revision used for the before/after sweep in the PR evidence (for example `469b357` or `main` at a stated sha). | Codex | Low |

## Consolidated Verdict
NEEDS_REVISION

## Priority Actions

1. **[High, agreement A1]** Rewrite AC2 to match `verdict_token`: in `prepr`, `PASS`, `PASS_WITH_NOTES` and `FAIL` match in upper case only, and only the older tokens fold case. The new scan must check `verdict_token` before the `toupper` in `single_verdict` runs.
2. **[High, D1]** Define an alternation by the full phase token list, so a token followed by prose that contains another token is a verdict. Add the fixture `## Verdict: PASS` ... `## Final Verdict: FAIL - PASS criteria were not met`, expected exit 3.
3. **[Medium, agreement A2]** Run all three workflow synthesis writers in CI with stubs. Check the `SYNTHESIS_FAILED: synthesis output has conflicting verdicts` first line and the outer exit status. Check also that spec and design review still publish the usable individual reports.
4. **[Medium, agreement A3]** Reconcile AC13, sub-task 3 and Evaluation #10 / #13. List which checks run in CI and which are PR evidence only. Give AC12 and Evaluation #13 to a sub-task.
5. **[Medium, agreement A4]** Give the exact P3 grammar: the closing markdown characters that are stripped, horizontal whitespace, a non-word boundary after `if|when|unless|otherwise`, and case-insensitive matching.
6. **[Medium, agreement A5]** Add these fixtures: P3 `unless` and `otherwise`; `synthesis-review` conflict; `review`-phase `APPROVE` against `APPROVE_WITH_SUGGESTIONS`; placeholder followed by two conflicting verdicts.
7. **[Medium, D2]** Decide the `vline` selection for the same verdict repeated across H2 and H3. Align AC9 with that decision.
8. **[Medium, N1]** Handle the `- PASS - no Must Fix items` prompt text, and add a fixture for a synthesis that only echoes the prompt.
9. **[Medium, N2]** State the three-line lookahead limit in AC1, or remove it.
10. **[Medium, N3]** Split AC12 into checks per phase: reviewer reports in `review`, and the synthesis in `synthesis-review` with `Priority Actions`.
11. **[Medium, N4, N5]** Pad the `review_guard` wrapper fixtures above the 100-character minimum. Make the red-first check on `main` include the conflict and P3 cases.
12. **[Medium, N6]** Tell the CLO-946 pre-PR reviewers to quote fixture headings inside code fences, or accept the named failure as expected.
13. **[Low, N7-N11]** Define "the gate does not pass" for Pi and for the inline gate. Use a scalar counter instead of `length(array)`. Update the `review_classify` header comment. Keep the test script free of bashisms. Fix the comparison revision for the PR evidence.
