# CLO-797: Fix grouping prompt exceeding the model context window when a commit touches hundreds of files

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-797
**Plan File**: docs/plans/clo-797-grouping-prompt-budget.md
**Design Document**: docs/designs/clo-797-grouping-prompt-budget.md
**Started**: 2026-10-29
**Last Updated**: 2026-10-29

---

## Current Status: Implementation complete, validation gate pending

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

- **The message path for the 515-file synthetic tree estimates at ~144,118 tokens**,
  over the 131,072 window. Message gathering is unchanged by design (ST4), so on such a
  tree a grouping refusal followed by a fallback ends in `PromptTooLarge` instead of a
  provider 400. Grouping itself fits, so AC1 holds.
- **Window sources**: Haiku 4.5 (200K) is from the official docs. Gemini 3.5 Flash-Lite
  and GPT-5.6 come from aggregator listings only.

---

## Next Steps

- Codex + Gemini validation gate
- PR phase
