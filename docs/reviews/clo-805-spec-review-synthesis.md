# Spec Review Synthesis: clo-805

**Reviewers**: Gemini (google-vertex/gemini-4-hydrogen#high), Codex (gpt-6.2-sol, effort high)
**Synthesized**: 2026-11-21
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| A1 | The seeded `config.toml` in the PTY fixture must have mode `0600`. The loader ignores a file with wider permissions, so the wizard would run as a first-time setup and never reach the confirmation. | High |
| A2 | The PTY fixture must be deterministic and offline. It needs its own `mktemp -d` `GCM_CONFIG`, an unset provider key env var (and `GCM_PROVIDER`), and a known path to the built-in fallback catalog. | High |
| A3 | AC9 and Evaluation row 6 must assert the exact persisted set `{A, B}` with `B` as default. A check that `B` is present lets a regression where `B` replaces `A` pass. | High |
| A4 | AC2 underspecifies what "reopens" means. Gemini found that `note()` placement relative to the loop is ambiguous. Codex found that the reopened picker starts unfiltered with the cursor on row 1, so an immediate SPACE toggles the wrong model. Both point to the same gap: the state of the reopened prompt. | High |
| A5 | Unit tests on `space_hint` cannot prove AC7. They do not show that the hint appears in Active/Error and disappears in Submit/Cancel. | Medium |
| A6 | Theme cleanup is not verified. Gemini flagged an unwind leak with manual `reset_theme()`. Codex noted that the radio comparison cannot detect a leaked checkbox override, because radios use a different renderer. | Medium |
| A7 | Cancellation and repeat paths need coverage: Esc, Ctrl-C, and No answered twice, each with an unchanged config afterwards. | Medium |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Codex Position | Claude Position |
|---|-------|-----------------|----------------|-----------------|
| D1 | Verdict | APPROVE_WITH_SUGGESTIONS | NEEDS_REVISION | NEEDS_REVISION. In A4, the specified test toggles the wrong row, so AC9 as written gives an invalid result. |
| D2 | Offline fixture route | Two options: a dummy key plus `GCM_*_BASE_URL=$MOCK_URL` (mock 501, then fallback), or no key plus skipping the prompt | No key, unset `GROQ_API_KEY`, skip the key prompt | Use the no-key route. It makes no HTTP call, avoids the retry delay, and does not need `start_mock`. Then remove the "fetch fails against the mock" wording from the Constraints so that they agree with Sub-task 3. |
| D3 | Error mapping for the new `confirm` | Says the spec matches the other prompts, with no violation | Map `Interrupted` to `wizard_cancelled()` and other errors to `wizard_io()` | Mirror the mapping that the other prompts in `run_provider_wizard` use. If they split on `Interrupted`, the confirm must split too. I did not check the source, as instructed. |
| D4 | Sub-task independence | Sub-tasks 1 and 2 are independent | Both edit the same `interact()` call, so one task must own the integration | Codex is correct. Give the call-site integration to Sub-task 1 and make Sub-task 2 deliver only the theme and the helper. |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| N1 | `make check` and CI do not run the PTY script, and `acceptance.sh` reuses an existing release binary. The spec needs a required, non-skipped run against a fresh build. | Codex | High |
| N2 | Add a baseline check: the pre-fix binary must fail at the missing confirmation. This proves that the test detects the bug. | Codex | High |
| N3 | The Constraints (line 59, "fetch fails against the mock") contradict Sub-task 3 ("pass the key step"). Without a key, the fetch short-circuits before any request to the mock. | Gemini | High |
| N4 | `space_hint` does not take the prompt state. The Submit/Cancel gate then sits outside the tested function. | Gemini | Medium |
| N5 | Use an RAII `ThemeGuard` that follows the `EchoGuard` pattern (`src/config/facade.rs:489`), and check at compile time that `cliclack::reset_theme` and the `Theme` methods exist in 0.5.4. | Gemini | Medium |
| N6 | The confirmation default (Yes or No) is a product decision. If the default is Yes, a second ENTER accepts the old set. Document that behavior and test it. | Codex | Medium |
| N7 | Pagination limit in cliclack: after a scroll past 15 rows, narrowing the filter can hide the highlighted row, so the hint does not render. Limit AC7 to rendered rows and document this. | Codex | Medium |
| N8 | The `expect` script must wait for the filtered redraw before it sends SPACE. cliclack redraws with ANSI sequences and carriage returns. | Gemini | Medium |
| N9 | The AC7 example `(recommended; SPACE to select)` uses a hint that `wizard_model_hint` never emits. Use `(not in live catalog; SPACE to select)`. | Gemini | Low |
| N10 | `README.md:208-210` describes ENTER saving directly. The spec must state whether the README is in scope. | Gemini | Low |
| N11 | The loop needs `initial_enabled.clone()`, because `initial_values` takes the `Vec` by value. Also, the line range `226-247` is off by a few lines (the block is `231-248`). | Gemini | Low |

## Consolidated Verdict
NEEDS_REVISION

## Priority Actions

1. **Define the reopen semantics in AC2** (A4): say whether `note()` repeats, and state that the picker reopens unfiltered with the cursor on row 1. Then rewrite AC9 and Evaluation row 6: wait for the reopened picker, type the filter again, wait for the redraw (N8), check that `B` is highlighted and unchecked, press SPACE and ENTER, and check that Default model follows with no second confirmation.
2. **Assert exact persistence** (A3): `models == {A, B}`, `model = B`, the process reached `eof`, and the exit code is 0.
3. **Specify the PTY fixture** (A1, A2, D2, N3): a private `mktemp -d` `GCM_CONFIG`, `chmod 600` on the seed, `env -u GROQ_API_KEY -u GCM_PROVIDER`, the no-key route with the key prompt skipped, and cleanup for each case. Remove the mock wording from the Constraints.
4. **Make the PTY run count** (N1, N2): require a non-skipped run against a fresh build, plus a baseline failure on the pre-fix binary.
5. **Fix the hint and theme design** (A5, A6, N4, N5): pass the prompt state into `space_hint` and test all four states. Use an RAII theme guard, and check that the cliclack 0.5.4 API exists.
6. **Settle the decomposition and error mapping** (D3, D4): one sub-task owns the `interact()` integration, and the confirm error mapping mirrors the sibling prompts.
7. **Add cancel and repeat coverage** (A7): Esc, Ctrl-C, and No twice, each with the config file unchanged afterwards.
8. **Record the product decisions** (N6, N7): the confirm default and what a second ENTER does, plus AC7 limited to rendered rows with the pagination limit documented.
9. **Low-cost fixes** (N9, N10, N11): the AC7 hint example, the README scope statement, the `clone()` note and the line range.
