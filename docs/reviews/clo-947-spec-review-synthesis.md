# Spec Review Synthesis: clo-947

**Reviewers**: Gemini (google-vertex/gemini-4-hydrogen#high), Codex (gpt-6.2-sol, effort high)
**Synthesized**: 2026-11-20
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| A1 | AC3 has a second branch: `mv` exits 0, but `<target>` is not a regular file after the move. Neither Decomposition #2 nor the Evaluation table has a test for this branch. Gemini proposes a test case: `<src>` is a directory and `<target>` does not exist. | High |
| A2 | AC5 says that "the `write_reviews` step exits 1". In `pre-pr-validation.toml` (line 338-340), that step is named `write_reports`. AC5 must name both steps. | Low |
| A3 | The spec does not show that the caller sees the `REVIEW_PUBLISH_FAILED:` line on stderr. The documented Pi entry point is `lok workflow run pre-pr-validation` (`.pi/orchestrator/phases/implement.md:214-218`). The spec's probe used `lok run --verbose`. Both reviewers ask for tests through the documented command. They do not agree on how serious the gap is (see D1). | High |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Codex Position | Claude Position |
|---|-------|-----------------|----------------|-----------------|
| D1 | Whether the stderr diagnostic reaches the caller | Medium and open. Check whether lok shows failed-step stderr without `--verbose`, or add `--verbose` to Evaluation #10 and #11. | Blocking. Codex ran a probe on lok `20261107.0.2` and cites `lok/src/utils.rs:185`. Lok classifies failed-step stderr and cuts unclassified summaries to 80 characters. A slug that contains `429` replaced the whole line with `rate limited (...)`. Writing to stderr does not guarantee that the prefix, the path and the reason reach the caller. | I agree with Codex. Codex gives a probe result and a source location. Gemini gives a question that has not been tested. `--verbose` does not change the classification or the 80-character limit. AC5 and AC10 must describe what the caller sees, and a probe must confirm it with a long path and a `429` slug. I did not check the lok source myself. |
| D2 | Verdict | APPROVE_WITH_SUGGESTIONS | NEEDS_REVISION | NEEDS_REVISION. D1 affects the main claim of the ticket, that the failure has a name. |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| N1 | Combined failure: the lok diff publication fails, and the synthesis destination is also a directory. The `REVIEW_INPUT_FAILED: see ...` message goes to stdout, and lok dropped stdout in the probe. In that case the diff reason exists only in the run's internal `input-failed` marker (`pre-pr-validation.toml:63`, `:396`). The spec must say where the caller finds the reason, and a test must cover this case. | Codex | High |
| N2 | The inline wrapper's exit boundary is not clear. In a probe, the child exited 1 or 3, the wrapper printed `gate exit 1` or `gate exit 3`, and the outer shell returned 0. AC6 and AC7 must state the child status, the printed status, that no success lines appear, and that the directory stays unchanged. A reference to CLO-949 must cover the outer status. | Codex | Medium |
| N3 | `implement.md:133` has two `review_publish` calls on one line. `grep -c` will count 2, not 3. The wiring check must count occurrences, for example with `grep -o ... \| wc -l`. | Gemini | Medium |
| N4 | The end-to-end tests are not exact enough. They must use valid stub review and synthesis responses and log model invocations for the diff failure. They must tell the writer's `usable=` output apart from the earlier `review_status` output. They must also assert that the rejected directory gains no file and that the `reports published` marker is absent. | Codex | Medium |
| N5 | Evaluation #13 must cover a partial publication: `$CR` succeeds and `$GR` is a directory. The gate must still exit 1 with `REVIEW_PUBLISH_FAILED:`. | Gemini | Medium |
| N6 | The decomposition understates its dependencies. The wiring tests pass only after both wiring sub-tasks land. The reader-doc sub-task edits `pre-pr-validation.toml`, which the workflow sub-task also edits, and it depends on the revised diagnostic contract. | Codex | Low |
| N7 | If `mv` fails with empty stderr, the line ends in `mv failed: `. The helper must fall back to a text such as `exit $rc`. | Gemini | Low |
| N8 | `[ -f "$target" ]` is true for a symlink to a regular file. `[ ! -L ]` is a stricter check. All current callers stage regular files, so either check works today. | Gemini | Low |
| N9 | The wiring check matches only `mv -f`. A plain `mv` added later will not be caught. | Gemini | Low |
| N10 | The gate-level tests must show both cases of a successful publication: a new file and a replaced file. | Codex | Low |

## Consolidated Verdict
NEEDS_REVISION

## Priority Actions

1. **Fix the diagnostic route (A3, D1).** Revise AC5 and AC10 to state what the caller sees. Use a route that is confirmed against lok's stderr classification and its 80-character limit. Probe it with `lok workflow run pre-pr-validation` and a normal path, a long path and a slug that contains `429`.
2. **Test the AC3 post-move branch (A1).** Add the directory-as-`<src>` case to Sub-task 2 and to the Evaluation table.
3. **Define the combined failure (N1).** Say where the caller finds the diff reason when the synthesis destination is also a directory, and add a test for it.
4. **Make the inline exit contract exact (N2).** State the child status, the printed `gate exit N`, that no success lines appear, and that the directory stays unchanged. Refer to CLO-949 for the outer status.
5. **Make the tests exact (N3, N4, N5).** Count occurrences in the wiring check. Use valid stubs and log invocations. Check `usable=` only in the writer. Assert no new file and no `reports published` marker. Add the `$GR` partial-publication case.
6. **Correct AC5's step name (A2).** Name both `write_reviews` and `write_reports`.
7. **Low-priority items (N6-N10).** State the decomposition dependencies, add the empty-stderr fallback, decide between `-f` and `! -L`, decide whether the wiring check also matches plain `mv`, and test both a new file and a replaced file at gate level.
