# Spec Review Synthesis (Round 3): clo-945

**Synthesized**: 2026-11-19
**Reviewers**: Gemini (google-vertex/gemini-4-hydrogen#high), Codex (gpt-6.2-sol, effort high)
**Reviewed spec sha256**: 1fdf4ba2d04b (after the third owner review)
**Revised spec sha256**: d9c54fcfb20a
**Pipeline**: manual run of the fixed reviewer commands

---

## Reviewer Status

Both external reviewers produced a real review and exited 0. Codex reported `model: gpt-6.2-sol` and `reasoning effort: high`. The Claude fallback was not needed.

Both verdicts were **NEEDS_REVISION**.

## Agreement (High Confidence)

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | The pre-PR fallback prompt still says "run `git diff main...HEAD`" (`pre-pr-validation.toml:168`), but AC26 gives the fallback no shell tool. | High | Defect 23; AC23 makes every pre-PR prompt, the fallback included, name the diff file and the task documents; Evaluation #18 checks it |
| 2 | The inline gate branches have no test (Gemini: zero reports and Codex failed; Codex: development and specification tasks). | Medium | Evaluation #23 |
| 3 | Sub-task 4 (pre-PR) is too large. | Medium | Split into 4 (input and dispatch) and 5 (synthesis, publication and exit); 7 sub-tasks |

## Findings (Codex Only)

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | A validator with `replace_output` can turn a `REVIEW_FAILED` line longer than 200 characters into an apparent review. | Critical | Owner decision "restrict all": `[steps.validate]` keeps only `min_length(200)`; defect 14 no longer applies; Evaluation #8b |
| 2 | The Haiku validators and the synthesis are unrestricted Claude calls; the pre-PR synthesis asks for repository grounding. | High | Defect 22; AC26 (b)-(e): synthesis is a shell step with `--tools ""`; Evaluation #22 probes both commands |
| 3 | AC9 promised a rollback that per-file renames cannot give. | High | AC9 states the real guarantee: staged writes, synthesis renamed last, a rename failure exits non-zero; callers trust reports only on exit 0; Evaluation #8c |
| 4 | Pi's extension accepts optional report paths, so the AC22 claim that Pi blocks partial reviews is too wide. | Medium | AC22 and AC14 narrowed: Pi's phase instructions, not its extension, require both reports |
| 5 | A model can write `NO_REVIEWS_AVAILABLE` itself. | Medium | AC25: the token is writer-only; Evaluation #8d and #14 |
| 6 | The health-check deadline and the short-circuit reason are not specified. | Medium | AC7 covers the health checks; AC12 and AC13 carry the reason |

## Findings (Gemini Only)

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | `fallback_status` needs `min_deps_success = 0`, or it is skipped when the fallback is skipped. | High | Not applied as written: lok records no result for a step that its `when` skipped (`lok/src/workflow.rs:1631-1638`). AC8 now cites this |
| 1a | `synthesis` in `design-review` needs a `when`. | High | Applied in all three workflows, with `steps.synthesis is defined` guards (AC8) |
| 1b | The inline gate writes reviewer output straight into `docs/reviews/`, so a failed run leaves no header and no failure layout. | Medium | AC16: output to per-run temporary files, then the AC9 layout through a rename; sub-task 6; Evaluation #23 |
| 2 | A shell-step `claude -p` needs `env -u CLAUDECODE`. | High | Not applied: the probe ran with `CLAUDECODE=1` and worked. AC26 (d) records it |
| 3 | `claude_fallback` as a shell step needs the AC7 timeout and `retries = 0`. | Medium | AC7 covers every shell step that starts a CLI |
| 4 | AC14 is overloaded and the zero-review override has no history action. | Medium | AC14 rewritten as a list (a)-(d); action `validation_override_no_reviews` in `orchestrate.md` and the template |
| 5 | Edge case 2 has no test row; Evaluation #3 misses `--auto` and `[backends.ollama]`; the #14 fixture needed reshaping. | Low | Evaluation #5, #3 and #14 updated |
| 6 | Sub-tasks 2-4 must assemble the fallback prompt with `shell_escape`. | Low | Sub-tasks 2-5 |

## Disagreement

None on substance. The two Gemini claims that the lok source and the probe disproved are listed above with their evidence.

## Flagged, Not Applied

- Concurrent runs on one slug: stated as an assumption in the workflow header comments, not built.
- Pi decision tests with each reviewer missing: the guarantee was narrowed instead, because `.pi/` is out of scope.

## Verdict

**NEEDS_REVISION**, resolved by the revision on 2026-11-19 (sha d9c54fcfb20a). The owner chose to restrict every Claude call, which relaxes the Must "keep `[steps.validate]`" to "keep its heuristic check".
