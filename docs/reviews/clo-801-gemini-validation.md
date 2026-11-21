# Pre-PR Validation: clo-801

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Validated**: 2026-11-21
**Pipeline**: implement.md validation gate

---

## Verdict: PASS_WITH_NOTES

All implementation acceptance criteria (AC1-AC9 and AC12) are accurately and cleanly implemented across `/Users/mk/Code/gcm--feat-clo-801-group-index/src/diff.rs`, `/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs`, `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/`, `/Users/mk/Code/gcm--feat-clo-801-group-index/src/budget.rs`, `/Users/mk/Code/gcm--feat-clo-801-group-index/src/cache.rs`, `/Users/mk/Code/gcm--feat-clo-801-group-index/src/main.rs`, and `/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs`. No correctness regressions, security issues, or library-surface leaks were found by static inspection. One automated integration test specified in Evaluation Row 16 is missing, and the owner-run live benchmarks (Sub-task 5: AC10, AC11, Ollama cloud run) remain open as expected.

*(Note: This review was performed by static inspection of the spec, branch diff, and source files; `cargo test`, `cargo clippy`, and `scripts/check-public-surface.sh` were not executed in this read-only review environment.)*

## Findings

- **MEDIUM - Missing `tests/grouping_budget.rs` capture case for Anthropic `stop_reason: max_tokens` fallback (Evaluation Row 16)**
  - In `/Users/mk/Code/gcm--feat-clo-801-group-index/docs/specs/2026-11-21-clo-801-group-index.md` (Evaluation Row 16), verification requires both the existing `extract_tool_use_input_max_tokens` unit test in `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/anthropic.rs` and a capture test in `/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs` pinning that `stop_reason: "max_tokens"` on the plan call falls back to a single commit.
  - `/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs` tests the 515-file grouped `tool_use` success and the 2,000-file `OutputCap` refusal, but does not include a `stop_reason: "max_tokens"` capture case. The production fallback path works (`extract_tool_use_input` returns `ErrorKind::Deserialize` and `build_plan` maps it to `BuildError::Fallback`), so this is a test-coverage gap rather than a runtime bug.

- **LOW - Provider payload unit tests do not explicitly assert the integer item schema or `"group by \"i\""` header (Evaluation Row 17)**
  - In `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/anthropic.rs`, `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/gemini.rs`, `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/groq.rs`, `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/openai.rs`, and `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/ollama.rs`, the test `ctx()` helpers still use `[{"path":"a.rs","xy":" M"}]` without `"i"`, and the payload tests only check `properties["groups"].is_object()` / `responseSchema["type"] == "OBJECT"` rather than checking `items.type == "integer"` / `"INTEGER"` or `"group by \"i\""`. At runtime, all six providers pull directly from `GROUPING_SYSTEM_PROMPT`, `grouping_user_content`, `plan::schema()`, and `plan::gemini_schema()`, which are updated and tested in `/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs`.

- **LOW - `index_echo_tokens` allocates a `Vec<String>` and joined `String` (`0..files`)**
  - `/Users/mk/Code/gcm--feat-clo-801-group-index/src/budget.rs` (`index_echo_tokens`, lines 423-426) allocates `(0..files).map(|i| i.to_string()).collect::<Vec<_>>().join(",")`. This matches AC8 literally and is fast for normal repositories, though an arithmetic byte-length calculation would compute the identical ASCII `len.div_ceil(2)` result in O(1) memory.

- **LOW - `cache::` unit test relies on golden-digest change rather than an end-to-end v3 cache-file miss test (Evaluation Row 22)**
  - `/Users/mk/Code/gcm--feat-clo-801-group-index/src/cache.rs` (`fingerprint_version_is_folded_in`, lines 373-390) bumps `FINGERPRINT_VERSION` and `FINGERPRINT_VERSION_GOLDEN` to `4` with the updated SHA-256 digest, proving v3 and v4 fingerprints differ. There is no separate `cache::load` test staging a v3 cache file on disk, though `cache::load` rejects any `cf.fingerprint != fingerprint(...)`.

## Missing Items

- **Evaluation Row 16 (`tests/grouping_budget.rs` capture test)**: Add a binary capture test where Anthropic returns `"stop_reason": "max_tokens"` on the plan call followed by an `end_turn` fallback message, asserting `requests.len() == 2` and single-commit fallback.
- **AC10 (live Vertex 515-file run + `main` baseline)**, **AC11 (live Anthropic 515-file run + `main` baseline)**, and **Sub-task 5 / Evaluation Row 28 (live Ollama cloud passthrough run + results appended to `/Users/mk/Code/gcm--feat-clo-801-group-index/docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md`)**: Owner-run checks requiring Vertex ADC, Anthropic, and Ollama Cloud credentials, intentionally left open per Section 2 of `/Users/mk/Code/gcm--feat-clo-801-group-index/docs/specs/2026-11-21-clo-801-group-index.md`.

## Recommendations

1. Add the missing Anthropic `stop_reason: "max_tokens"` capture test in `/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs` (Evaluation Row 16) to verify the 2-request fallback path end-to-end.
2. Update the provider payload tests in `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/{anthropic,gemini,groq,openai,ollama}.rs` to include `"i": 0` in their `ctx()` fixtures and assert `items["type"] == "integer"` (or `"INTEGER"`) and `"group by \"i\""` in the serialized payload.
3. Keep the pre-existing `output::fallback_raw_code` substring-classifier bug (`DuplicateFile` whose path contains `"plan parse error"`) documented as a follow-up item in the PR description, as required by the spec's Must-not section.
4. Include the exact owner-run commands for AC10, AC11, and the Ollama cloud run in the PR description so Sub-task 5 can be executed and recorded in `/Users/mk/Code/gcm--feat-clo-801-group-index/docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md`.
