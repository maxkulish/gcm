# CLO-797: Fix grouping prompt exceeding the model context window when a commit touches hundreds of files

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-797
**Plan File**: docs/plans/clo-797-grouping-prompt-budget.md
**Design Document**: docs/designs/clo-797-grouping-prompt-budget.md
**Started**: 2026-10-29
**Last Updated**: 2026-10-29

---

## Current Status: Validation passed with notes, ready for the PR

**Overall Progress**: 100% (7/7 sub-tasks)
**Branch**: fix/clo-797-grouping-prompt

---

## Session Log

### Session 1 - 2026-10-29

| Sub-task | Commit | Summary |
|---|---|---|
| Baseline | 9000f5f | Golden file pins message gathering before the change |
| ST1 | 382e83a | One `{path, xy, old_path?}` inventory replaces the file list and the status block |
| ST2 | 044fd01 | Untracked files past the read cap share one grouping trailer |
| ST3 | 6fe4920 | `src/budget.rs`: window table, env override, grouping and message budgets |
| ST4 | fb4d33f | Sacrifice ladder in `gather_for_grouping`: Full, Shrunk, HeadersOnly, PathsOnly, CannotFit |
| ST5 | c576368 | `ensure_prompt_fits` guards the four call sites; `GcmError::PromptTooLarge` |
| ST6 | 5d76db7 | Ladder and default-window warnings on stderr through the CLO-798 routing |
| ST7 | 23533f4 | `tests/grouping_budget.rs`: AC1, AC2, AC4, zero-request cases, output-cap fallback |
| ST3 fix | 53139a6 | Groq `qwen/qwen3` and `openai/gpt-oss-` prefixes at 131,072 from Groq's `/models` |
| D5 addendum | ed5e019 | Message refusal measures at 4 bytes per token; grouping keeps 2 |
| Codex fixes | 0700eae | Terminal rung decided on paths and stat; cut untracked files counted; CannotFit-then-fallback test |
| Codex fix | ee8de89 | Omitted-body marker dropped past the byte ceiling too |

---

## Regression Guard (red before fix)

`ac1_grouping_payload_fits_the_window` was copied into a temporary worktree at
pre-fix `main` (7c7cb28) and run there:

```
---- ac1_grouping_payload_fits_the_window ----
grouping payload is ~212714 estimated tokens, over the 131072-token window
test result: FAILED. 0 passed; 1 failed
```

The same test passes on the branch.

The generous-refusal tests were run on 53139a6 before the change. All three sent zero
requests because the pre-flight refused each message:

```
single_commit_message_under_the_generous_estimate_is_sent  left: 0  right: 1
provider_context_window_error_stays_actionable             left: 0  right: 1
invalid_plan_falls_back_to_a_sent_message                  left: 1  right: 2
```

They pass on ed5e019.

The two ladder regressions for the first Codex review were run with the fixes removed
from `src/diff.rs` in a temporary worktree at edf6d3f. Both failed:

```
ladder_counts_truncated_untracked_files_as_reduced ... FAILED
ladder_paths_only_drops_the_marker_when_only_paths_and_stat_fit ... FAILED
```

---

## Technical Decisions

- **Budget tests live in `budget::tests`.** The plan named `models::tests::context_window`
  and `diff::tests::estimate`; `models.rs` is library code, so the table and estimator
  stayed binary-side in `src/budget.rs` to keep the public surface unchanged.
- **Groq gpt-oss keeps the reasoning allowance.** `include_reasoning: false` hides the
  reasoning but the model still spends tokens on it; only qwen (`reasoning_effort: none`)
  gets 0.
- **An output-cap rejection falls back with `raw_code: PromptBudget`**, the same code as a
  grouping prompt that cannot fit.
- **The cached-regeneration test seeds the cache with a real commit** of group 1 of a
  two-group plan, because a fresh plan with a null group-1 message is rejected as invalid.
  The test and the output-cap fallback test need `git commit -S` and skip without it.

---

## Important Findings

- **The 2-byte estimate refused messages the provider accepts.** The 515-file `--all`
  message (281 KB) estimated at ~144,118 tokens and was refused, although released 0.8.3
  sent it and Groq answered. The owner approved a generous 4-bytes-per-token estimate for
  message refusal only (design D5 addendum, ed5e019). The live dry-run on ed5e019 sent
  and got answers for `--all` and the grouped fallback on qwen/qwen3.8-27b and
  openai/gpt-oss-120b.
- **The generous estimate is a heuristic.** It undercounts non-ASCII text on purpose (the
  CLO-798 context-window classifier is the backstop) and can still refuse text above 4
  bytes per token.
- **Window sources**: Haiku 4.5 (200K) is from the official docs. Gemini 3.5 Flash-Lite
  and GPT-5.6 come from aggregator listings only.

---

## Next Steps

- Codex validated 53139a6: FAIL (docs/reviews/clo-797-codex-validation.md). The owner
  chose to fix the in-design findings and defer the emoji undercount to CLO-921.
- Codex re-validated 0700eae: PASS_WITH_NOTES (docs/reviews/clo-797-codex-validation-2.md).
  Its one new LOW finding is fixed in ee8de89. Gemini CLI is not installed and is
  recorded as unavailable.
- Follow-ups: CLO-921 (emoji undercount, including the path-echo reserve and the output-cap
  check); primary sources for the Gemini and OpenAI window entries.
- `/session:wrap`, then the PR phase.
