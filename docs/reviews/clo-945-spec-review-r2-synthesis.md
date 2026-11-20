# Spec Review Synthesis (Round 2): clo-945

**Synthesized**: 2026-11-19
**Reviewers**: Gemini (google-vertex/gemini-4-hydrogen#high), Codex (gpt-6.2-sol, effort high)
**Reviewed spec sha256**: 5c434bc8f048 (after the second owner review)
**Revised spec sha256**: dd1f31a7190b
**Pipeline**: manual run of the fixed reviewer commands

---

## Reviewer Status

Both external reviewers produced a real review and exited 0. Codex reported `model: gpt-6.2-sol` and `reasoning effort: high`. The Claude fallback was not needed.

Gemini gave **APPROVE_WITH_SUGGESTIONS**. Codex gave **NEEDS_REVISION**.

## Agreement (High Confidence)

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | Failure details have no channel: lok templates expose only `output` and `success`, so the writer cannot read the stderr of a step. | High | AC9: each reviewer writes stderr to `.lok/local/review-runs/<slug>/<reviewer>.stderr` |
| 2 | Diff preparation can fail (no `main`, failed `git diff`, failed write) without a named outcome. | Medium | AC23: fallback to `origin/main`, else `REVIEW_INPUT_FAILED:`, no reviewer starts, lok exits non-zero; Evaluation #20 |
| 3 | The inline gate tests miss the zero-review and pause paths. | Medium | Evaluation #20 covers the input failure; #11 and #8c cover zero reviews |

## Findings (Codex Only)

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | Pre-PR can succeed with one usable review and a failed or malformed synthesis; the fallback is not classified before synthesis reads it. | High | AC14: `SYNTHESIS_FAILED:` synthesis and exit 1 in pre-PR; AC8: `fallback_status` step; Evaluation #8e |
| 2 | Pi's checklist requires both external reports and `codex_validated: true`, so a one-review run that this workflow passes is still blocked by Pi. | High | AC14 states the outcome: the workflow passes, Pi blocks by its own rule, which this change keeps |
| 3 | AC18 lets a design report pass without the `Actionable Feedback` section that the phases read. | Medium | AC18 requires an `Actionable Feedback` heading; Evaluation #8g |
| 4 | AC9 startup deletion removes tracked checkpoint reports before a replacement exists. | Medium | AC9: the health check resets only the run directory; the writer renames complete files over the targets; Evaluation #8f |
| 5 | Evaluation #4 expects a one-line failure file while AC10 requires a header. | Low | AC9 defines the failure-report layout; Evaluation #4 updated |
| 6 | Sub-tasks 2-4 are large for two hours. | Low | Not changed: the shared guard in sub-task 1 owns the format; noted for the plan |

## Findings (Gemini Only)

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | The writer must keep the guard's `REVIEW_FAILED:` line when the validator also failed the step. | High | AC9 states this precedence |
| 2 | `review_status` must run when both reviewers fail; `synthesis` `when` must not read a skipped `claude_fallback`. | High | AC8: `review_status` ends with exit 0; `synthesis` reads only the status steps |
| 3 | `retries = 3` with a 240 s child timeout cannot fit the 5 m step. | Medium | AC7: reviewer steps set `retries = 0`; Evaluation #21 |
| 4 | Evaluation #1 misses an opencode `--dir` on a continued line. | Medium | Evaluation #1 adds a second `rg` that excludes only `lok run` |
| 5 | `.lok/prompts/validate-review-output.md` holds old verdict names and is unused. | Low | Not in scope; already a follow-up |

## Disagreement

The verdicts differ (APPROVE_WITH_SUGGESTIONS against NEEDS_REVISION). The Codex findings 1-4 describe real gaps in the gate outcome, so the revision applied them. No suggestion contradicted a maintainer decision.

## Verdict

**NEEDS_REVISION**, resolved by the revision to sha dd1f31a7190b.
