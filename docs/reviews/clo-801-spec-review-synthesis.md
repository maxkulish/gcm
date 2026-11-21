# Spec Review Synthesis: clo-801

**Reviewers**: Gemini (google-vertex/gemini-4-hydrogen#high), Codex (gpt-6.2-sol, effort high)
**Synthesized**: 2026-11-21
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | AC7 breaks an existing test. `output_cap_rejection_falls_back_to_one_message_request` (`tests/grouping_budget.rs:426`) expects Anthropic to refuse grouping for 515 files. Under AC7 that request goes through, so the test fails. The spec does not list `tests/grouping_budget.rs` in its scope, decomposition or evaluation table. Both reviewers want two cases: 515 files now sends the `tool_use` grouping request, and a separate over-cap case (roughly 2,000 files) still sends no grouping request. | High |
| 2 | AC6 contradicts itself. It says the existing tests pass unmodified, but bumping `FINGERPRINT_VERSION` to 4 requires updating `FINGERPRINT_VERSION_GOLDEN` and its digest (`src/cache.rs:372-388`). AC6 needs to say which test expectations may change and which must stay. Gemini adds that `plan_error_display_is_distinct` grows from 7 to 8 variants. Codex adds that the path-string parser tests must resolve the new wire type before they compare paths. | Medium |
| 3 | Index conversion needs explicit bounds handling. Gemini proposes `FileRef::Index(usize)` and `UnknownIndex(usize)` so that values above `usize::MAX` fail at parse time. Codex asks for checked conversion and lookup, plus an overflow test. Both reviewers mean the same thing: an index must never panic or wrap. | Medium |
| 4 | AC11 (Ollama compatibility) has no test. Section 2 claims unit-test coverage, but no evaluation row covers a prompt-only reply that uses integer indices. The two reviewers disagree on what kind of test closes the gap (see Disagreement #3). | Medium |
| 5 | The pre-flight output-cap check counts only the index echo. Near 1,500 files the index list fits under 4,096 tokens, but the full reply (indices, summaries and commit message) can exceed `PLAN_MAX_TOKENS` and stop on `max_tokens`. Both reviewers agree on this fact. They disagree on whether the spec must act on it (see Disagreement #2). | Low-Medium |
| 6 | Two edge cases are listed in "Edge cases to verify" but have no row in the evaluation table: a redacted path in the inventory (`privacy.prepare_grouping`), and a renamed file that resolves by index to its new path with `old_path` ignored. Both reviewers also want at least one full-binary test that runs an integer-index plan, through `scripts/acceptance.sh` or a CLI integration test. | Low |
| 7 | The defensive parser's recovery rules do not change. `normalize_group` does not touch `files` elements, so recovery still checks `groups` before `commits` and still applies the `message` alias only to the first group (CLO-517). Codex asks for a test that pins this. | Low |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Codex Position | Claude Position |
|---|-------|-----------------|----------------|-----------------|
| 1 | Verdict | APPROVE_WITH_SUGGESTIONS. The design is sound, and the problems are test and compile details. | NEEDS_REVISION. The live checks cannot be run as written, and the test plan does not account for the existing Anthropic and cache test expectations. | NEEDS_REVISION. The aggregation rule requires it, and the substance supports it: AC6 contradicts the golden test, and AC7 contradicts an existing integration test. All of these are spec edits, not design changes, so one revision round is enough. |
| 2 | Output-cap boundary near 1,500 files | Note it only. A `max_tokens` stop already falls back to a single commit, and the gap existed in the original design. | Define the policy. Say whether the check estimates the full completion or is a heuristic, and test the boundary and truncation behavior. | Keep the formula. Add one sentence saying that the check is an echo-only heuristic and that a `max_tokens` stop falls back to a single commit. Add one test that pins the fallback. This costs little and stops a later reviewer from treating it as a bug. |
| 3 | Type of test for AC11 | A unit test that parses a fenced, prompt-only reply with integer indices. | An Ollama cloud smoke test that Max runs himself, proving a real model follows the integer prompt. | Require the unit test, because it proves the parser. Add the smoke test as an owner-run step in Sub-task 5, with the same "credentials unavailable" fallback as AC9 and AC10. The parser test alone cannot show that a weak model follows the new prompt. |
| 4 | Change to the `Provider::generate_plan` signature | Not a violation, because the trait is binary-internal under ADR-002. | The return type changes an internal contract. Move it from "Prefer" to an explicit requirement and update the trait documentation. | Both are correct. It is not an ADR-002 violation, but it is a contract change, so the spec must state it as a requirement and not as a preference. |
| 5 | How resolution errors get classified | Keep the current substring classifier, and make sure the `UnknownIndex` message does not contain "plan parse error". | Malformed wire data stays `ErrorKind::Deserialize`, and resolution and partition failures map to `PlanValidation` directly, without the substring classifier. | The spec must state the split as an outcome: wire errors are `Deserialize`, resolution errors are `PlanValidation`. Leave the mechanism to the implementation. The way the existing classifier can mislabel `DuplicateFile` (below) existed before this change, so it belongs in a follow-up, not in this change. |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | AC9 and AC10 have no exact pass condition. `--plan-only` returns `status: "plan"` both after a fallback and on a cache hit. The check must require `mode: "plan_only"`, `cached: false`, and a valid partition of all 515 expected paths. AC10 must also require that the request uses `max_tokens: 4096`. | Codex | Medium |
| 2 | The timing method in the spec cannot verify AC9. JSON mode disables the progress ticker, and `finish()` reports no duration. Measure the time around `generate_plan` or the total CLI wall time. Use the same provider and model on both builds: the 85.5s baseline ran on `google`, while AC9 targets `vertex`. | Codex | Medium |
| 3 | `anthropic.rs:257-259` deserializes the `tool_use` input and then calls `serde_json::to_string` on it. If `PlanReply`, `ReplyGroup` and `FileRef` do not derive `Serialize`, that code does not compile. The alternative is to serialize the raw `input` value directly. | Gemini | Medium |
| 4 | A wrong mapping can still pass validation, because any permutation of valid indices forms a valid partition. Add a fixture whose inventory is not in alphabetical order and whose group membership is known. Otherwise an off-by-one error or a lookup against the wrong slice goes undetected. | Codex | Medium |
| 5 | `tests/grouping_budget.rs:460` checks that stderr contains "plan output cap". Any new wording of `PromptCause::GroupingOutputCap` must keep that phrase. | Gemini | Low |
| 6 | Row 7 tests only `-1` and `1.5`. AC4's "any other JSON type" (`true`, `null`, `{}`) has no test. The spec also does not say whether `1.0` is accepted or rejected. | Gemini | Low |
| 7 | Error order changes. If `resolve` runs before `validate`, a plan with both `MissingFirstMessage` and an out-of-range index reports `UnknownIndex` instead. Both errors map to `PlanValidation`, so the result is the same. The spec only needs to choose an order if a stable error order matters. | Gemini | Low |
| 8 | The spec does not say how to test Row 14 and Row 16. For Row 14, use a `TcpListener` with `GCM_ANTHROPIC_BASE_URL`, or test `extract_tool_use_input` and the `PlanReply` parse separately. For Row 16, use `Repo::at`, a temporary repo and a stub `Provider`, or extract a helper that does resolve and validate. | Gemini | Low |
| 9 | A string index such as `"0"` stays a literal path, fails with `UnknownFile`, and falls back to a single commit. This is the intended tradeoff that keeps numeric filenames working. It fails closed, and the spec could state it. | Gemini | Low |
| 10 | Doc comments that still describe path echoing or the old `{path, xy, old_path?}` inventory: `anthropic.rs:141`, `budget.rs:26,337,346,423` and `diff.rs:20,142`. | Gemini | Low |
| 11 | Incorrect references in the spec: the second shortcut is at `anthropic.rs:257`, not `:255`, and the investigation doc names `~/Work/investigations`, not `~/Code/investigations`. | Gemini | Low |
| 12 | "Silently loses the grouping" overstates the problem, because the fallback is announced on stderr at the default log level. | Codex | Low |
| 13 | The existing substring classifier (`output.rs:301`) can mislabel a `DuplicateFile` error if the filename contains "plan parse error". This bug existed before this change. | Codex | Low (follow-up) |
| 14 | Give live verification its own owner and credential prerequisites, so the 2-hour estimate covers only the implementation work. | Codex | Low |

## Consolidated Verdict
NEEDS_REVISION

## Priority Actions

1. **Add `tests/grouping_budget.rs` to the scope, Sub-task 4 and the evaluation table.** Change the 515-file Anthropic case so it expects a successful `tool_use` grouping request with integer indices. Add an over-cap case (roughly 2,000 files) that expects no grouping request. Keep the "plan output cap" phrase in stderr. (Agreement #1, Novel #5)
2. **Rewrite AC6.** Name the expectations that may change: the fingerprint golden and its digest, the `PlanError` display count from 7 to 8, and the parser fixtures that now resolve before they compare. Keep the `validate*` tests and the `CACHE_FORMAT_VERSION` assertions unchanged. (Agreement #2)
3. **Make AC9 and AC10 executable.** Require `mode: "plan_only"`, `cached: false`, a full partition of the 515 paths, and `max_tokens: 4096` on the request. Measure wall time around the whole CLI run or around `generate_plan`. Use the same provider and model before and after. (Novel #1, #2)
4. **Settle the index type.** Use `FileRef::Index(usize)` and `UnknownIndex(usize)` with checked lookup. Add tests for overflow, `true`, `null`, `{}` and `1.0`. (Agreement #3, Novel #6)
5. **Make the wire-type contract explicit.** Move the `generate_plan` return-type change from "Prefer" to a requirement. Require `Serialize` on the wire types, or have `anthropic.rs:257` serialize the raw `input`. State the split: wire errors are `Deserialize`, resolution errors are `PlanValidation`. (Disagreement #4, #5, Novel #3)
6. **Cover AC11.** Add a unit test for a fenced, prompt-only reply with integer indices. Add an owner-run Ollama cloud smoke test in Sub-task 5. (Agreement #4, Disagreement #3)
7. **Close the remaining evaluation gaps.** Promote the redacted and renamed edge cases to table rows. Add a fixture whose inventory is not in alphabetical order. Add one full-binary run with an integer-index plan. Pin the CLO-517 first-group-only `message` alias. (Agreement #6, #7, Novel #4)
8. **State the output-cap policy in one sentence.** The check is an echo-only heuristic, and a `max_tokens` stop falls back to a single commit. Pin that fallback with a test. (Agreement #5, Disagreement #2)
9. **Minor fixes.** Correct the `anthropic.rs:257` reference and the investigations path. Soften "silently". Update the doc comments that still describe path echoing. Note how to test Row 14 and Row 16. (Novel #8, #10, #11, #12)

The `DuplicateFile` mislabel in the substring classifier (Novel #13) existed before this change, so it stays out of this spec.
