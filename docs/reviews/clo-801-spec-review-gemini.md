# Spec Review: clo-801

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Reviewed**: 2026-11-21
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is clear, self-contained, and matches the Linear task (`CLO-801`) and the evidence in `/Users/mk/Code/gcm--feat-clo-801-group-index/docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md`. All referenced symbols and line numbers match the codebase (`PLAN_MAX_TOKENS` at `src/provider/anthropic.rs:142`, `PromptBudget::grouping` at `src/budget.rs:426`, `PATH_ECHO_FACTOR` at `src/budget.rs:29`, `files_json` at `src/diff.rs:24`, `plan::schema()` at `src/plan.rs:310`, `plan::gemini_schema()` at `src/plan.rs:339`, `GROUPING_SYSTEM_PROMPT` at `src/provider/facade.rs:355`, `prepare_grouping` at `src/privacy/facade.rs:34`, `fallback_raw_code` at `src/output.rs:304`, and `FINGERPRINT_VERSION` at `src/cache.rs:28`).

Minor notes on accuracy and unstated assumptions:
- The spec cites `/Users/mk/Code/investigations`, whereas the investigation doc names `~/Work/investigations`.
- The spec cites `src/provider/anthropic.rs:255` for the second direct `Plan` deserialization shortcut; the actual `serde_json::from_value::<Plan>` call is at `:257`.
- The spec assumes adding `"i": <n>` to every `files_json` entry adds negligible input tokens (~4 KB / ~2K estimated tokens for 515 files), which is true relative to the tens of thousands of tokens saved in `completion_reserve`, though worth noting explicitly for tight-window models.

## 2. Acceptance Criteria Review

**Strong**:
- AC1-AC5 cleanly specify the inventory shape (`i` 0-based), strict/OpenAPI schema changes, prompt updates, mixed integer/string parsing, numeric string handling (`"12"` stays a path), rejection of negative/fractional/other JSON types as `PlanError::Parse`, and `UnknownIndex(n)` -> `PlanValidation` without panicking.
- AC6 and AC8 properly lock backward compatibility of `Plan`, `Group`, `validate*`, `CACHE_FORMAT_VERSION`, and Anthropic's `tool_use` direct paths, while bumping `FINGERPRINT_VERSION` to 4.
- AC9 and AC10 set measurable live targets (<=30s on default 60s timeout for Vertex, no `max_tokens` truncation or `GroupingOutputCap` refusal on Anthropic) with an explicit fallback plan if credentials are unavailable.

**Gaps**:
- **AC6 vs `PlanError` test**: AC6 states that `Plan`, `Group`, `validate`, `validate_cached`, `validate_partition`, and their existing tests pass unmodified. However, adding `PlanError::UnknownIndex` requires updating `plan_error_display_is_distinct` in `/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs:609-624` (which asserts 7 distinct `Display` strings), and bumping `FINGERPRINT_VERSION` requires updating `FINGERPRINT_VERSION_GOLDEN` and its SHA-256 hex digest in `/Users/mk/Code/gcm--feat-clo-801-group-index/src/cache.rs:378-388`. AC6 should clarify that `validate*` tests stay unmodified while `PlanError` and `cache` version golden tests update.
- **AC7 breaks an existing integration test**: `/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs:426-461` (`output_cap_rejection_falls_back_to_one_message_request`) uses 515 files (`large_tree`) with Anthropic and asserts that `OutputCap` rejects the call (`requests.len() == 1`, `fallback.raw_code == "PromptBudget"`). Under AC7, 515 files will no longer be rejected. The spec has no criterion or task updating `tests/grouping_budget.rs`.
- **AC11 verification gap**: AC11 mentions prompt-only replies (schema ignored, CLO-517 style) using either integers or paths, and Section 2 states AC11 is verified by `cargo test` unit tests, but neither Section 4 nor Section 5 includes a test for an Ollama passthrough prompt-only integer reply wrapped in markdown fences/prose or `commits` with a non-standard key mix (Evaluation row 9 only covers `commits` with integers).

## 3. Constraints Check

**Aligned**:
- Keeping `Group.files: Vec<String>`, all validator signatures, and `CACHE_FORMAT_VERSION` intact isolates the wire representation from staging, caching, and `--json` envelopes (`/Users/mk/Code/gcm--feat-clo-801-group-index/src/output.rs`).
- Mapping indices against `changed` in `main::build_plan` rather than redacted `ctx.files` prevents `--secret-scan=redact` from breaking grouping when a file path matches a secret rule.
- Keeping `Provider::generate_plan` binary-only aligns with `/Users/mk/Code/gcm--feat-clo-801-group-index/docs/adrs/002-library-boundary.md` and `/Users/mk/Code/gcm--feat-clo-801-group-index/scripts/check-public-surface.sh`.
- Not raising `PLAN_MAX_TOKENS` or `DEFAULT_TIMEOUT_SECS` ensures the fix solves the actual output-token root cause rather than masking it.

**Concerns**:
- **`u64` vs `usize` inconsistency**: Section 3 (Prefer) proposes `FileRef::Index(u64)`, whereas `PlanError::EmptyGroup(usize)` (`/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs:36`) and `changed.len()` (`usize`) are `usize`, and AC5 requires `PlanError::UnknownIndex(n)` for `n >= changed.len()`. If `FileRef` holds `u64`, `resolve` must handle `usize::try_from(n)` (e.g., on 32-bit targets or `u64::MAX`) without panicking. Prefer `FileRef::Index(usize)` and `PlanError::UnknownIndex(usize)` so out-of-range integers > `usize::MAX` fail cleanly at parse time and everything else is `usize`.
- **Implicit `Serialize` constraint on wire types**: `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/anthropic.rs:257-259` deserializes `tool_use` `input` and immediately re-serializes it via `serde_json::to_string(&plan)`. If `PlanReply`/`ReplyGroup`/`FileRef` do not derive `Serialize`, `anthropic.rs` will fail to compile unless `extract_tool_use_input` stops re-serializing on the shortcut path.
- **Preserving `"plan output cap"` wording in `PromptTooLarge::fmt`**: `/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs:460` asserts `stderr.contains("plan output cap")`. Rewording `PromptCause::GroupingOutputCap` in `src/budget.rs:368-374` must retain that phrase.

## 4. Decomposition Quality

**Well-scoped**:
- Sub-tasks 1-5 are logically partitioned by module (`diff.rs`, `plan.rs`, `provider/*`, `main.rs` + `budget.rs` + `cache.rs`, and the live re-measure). Each sub-task is small and well under 2 hours.
- Dependency ordering is explicitly stated, including noting that sub-tasks 2-4 must land together for the crate to compile cleanly.

**Issues**:
- **Missing integration-test sub-task / file**: `/Users/mk/Code/gcm--feat-clo-801-group-index/tests/grouping_budget.rs` is omitted from Sub-task 4 (and Section 1's file count of 13 files). `output_cap_rejection_falls_back_to_one_message_request` (`tests/grouping_budget.rs:426`) must be updated to use a larger file count (e.g., ~2,000 files, where `0..n` joined by `,` exceeds 4096 estimated tokens) and a new integration case should verify that 515 files on Anthropic now sends the grouping request.
- **Unmentioned test updates in Sub-task 4**: `src/cache.rs:378-388` (`FINGERPRINT_VERSION_GOLDEN` and the hardcoded digest) must be updated when bumping `FINGERPRINT_VERSION`, and `src/plan.rs:609` (`plan_error_display_is_distinct`) must be updated in Sub-task 2.
- **Sub-task 4 / Row 16 testability in `src/main.rs`**: `/Users/mk/Code/gcm--feat-clo-801-group-index/src/main.rs:1275-1291` currently has no `build_plan` tests, and `build_plan` requires `&Repo` and `&Privacy`. Sub-task 4 should note using `Repo::at` + a temporary git repo + a stub `Provider` (or extracting a resolve+validate helper) so Evaluation row 16 does not run into unexpected setup friction.

## 5. Evaluation Coverage

**Covered**:
- Rows 1-13, 15, and 17-20 give concrete inputs, expected outcomes, and exact `cargo test` / CLI commands covering happy paths, schemas, numeric strings, negative/fractional rejection, duplicate indices, `commits` alias, budget math, cache invalidation, and live runs.

**Gaps**:
- **Integration test `tests/grouping_budget.rs` missing**: No row verifies the end-to-end binary behavior of `large_tree` (515 files) on Anthropic sending the tool-use grouping request, nor updates the broken `output_cap_rejection_falls_back_to_one_message_request` test to an oversized index count (~2,000 files).
- **Row 14 (`Anthropic::generate_plan` unit test)**: `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/anthropic.rs` currently only unit-tests `extract_tool_use_input` (not `generate_plan` end-to-end, which hits the second shortcut at `anthropic.rs:87`). Either add a local `TcpListener` unit/integration test targeting `GCM_ANTHROPIC_BASE_URL` for `Anthropic::generate_plan`, or explicitly test both `extract_tool_use_input` and `serde_json::from_str::<PlanReply>`.
- **Row 7 missing non-number JSON types**: Row 7 tests `-1` and `1.5`, leaving AC4's `"or any other JSON type"` (`true`, `null`, `{}`) untested. Also consider `1.0` (JSON float equal to an integer).
- **Edge cases not in the numbered table**: Two items from "Edge cases to verify" (redacted path in inventory via `privacy.prepare_grouping`, and renamed file resolving to new path with `old_path` ignored) have no corresponding rows in the test table.
- **End-to-end index plan in `scripts/acceptance.sh`**: `/Users/mk/Code/gcm--feat-clo-801-group-index/scripts/acceptance.sh` only stages path-string plans in `$PLAN_FILE` (which now test the string fallback). Adding at least one integer-index case there (or in `tests/grouping_budget.rs`) would verify the full CLI path with integer indices.

## 6. Codebase Alignment

**Violations**:
- None against `Backend`/`Provider` trait contracts, `ErrorKind`, `GcmError`, `--json` raw codes, or ADR-002's library boundary (`/Users/mk/Code/gcm--feat-clo-801-group-index/src/lib.rs` does not export `plan`, `diff`, `budget`, `cache`, or `Provider`). Notice that `/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/facade.rs:139,164` (`resolve_schema` and `gemini_resolve_schema`) already uses `"type": "integer"` / `"type": "INTEGER"` across all six providers in the exact pattern proposed here.

**Alignment**:
- Preserves the frozen `output::fallback_raw_code` contract (`/Users/mk/Code/gcm--feat-clo-801-group-index/src/output.rs:304`) by ensuring `PlanError::UnknownIndex` does not contain `"plan parse error"`.
- Preserves `parse_defensive` recovery precedence (`groups` before `commits`, `message` -> `commit_message` gated to `groups[0]`) because `normalize_group` (`/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs:270`) does not inspect or rewrite `files` elements.
- Keeps `plan.rs` pure (no dependency on `git::ChangedFile`) if `plan::resolve` takes `&[String]` or `&[&str]`.

## 7. Blind Spots

1. **Broken existing test in `tests/grouping_budget.rs`**: `output_cap_rejection_falls_back_to_one_message_request` (`tests/grouping_budget.rs:426`) asserts Anthropic rejects 515 files at `PromptBudget::grouping`. This will fail immediately once AC7 is implemented.
2. **`resolve` vs `validate` error precedence**: If a malformed plan has both `MissingFirstMessage` (or `EmptyGroup(0)`) and an out-of-range index in `groups[1]`, calling `plan::resolve` before `plan::validate` changes the reported error from `MissingFirstMessage`/`EmptyGroup(0)` to `UnknownIndex(n)` (whereas `["ghost.rs"]` reports `MissingFirstMessage`/`EmptyGroup(0)` first). Both map to `PlanValidation`, so behavior is safe, but if deterministic error precedence per `src/plan.rs:363-375` is desired, `check_structure` + first-message check would run before `resolve` (or on `PlanReply` via a shared check).
3. **Anthropic `OutputCap` vs `FIXED_OUTPUT_OVERHEAD` near the 4096-token boundary**: `PromptBudget::grouping` checks `echo_tokens > cap` (`4096`), where `echo_tokens` only counts `"0,1,...,n-1"`. At ~1,500-1,750 files, `echo_tokens` is ~3,500-4,050 (`<= 4096`), so the pre-flight check passes, yet the actual reply plus summaries and commit message can exceed `PLAN_MAX_TOKENS = 4096` and hit `stop_reason: max_tokens` (which degrades cleanly to single-commit fallback, not a crash). This matches the pre-existing `OutputCap` design, so no change is strictly required, though it is good to be aware of.
4. **Stringified indices from weak-schema models (AC11)**: If an Ollama cloud passthrough model ignores the prompt and emits `["0", "1"]` (strings instead of integers), AC4 treats `"0"` as a literal path and `validate` rejects it with `UnknownFile("0")`, falling back to single-commit. This is an intentional tradeoff to keep literal numeric filenames like `"1"` working (Evaluation row 5), and is safe because it fails closed to single-commit fallback.
5. **Minor stale doc/comment references**: `src/provider/anthropic.rs:141` ("path-echo check"), `src/budget.rs:26,337,346,423`, and `src/diff.rs:20,142` (`{path, xy, old_path?}`) have doc comments referencing path echoing / old inventory shape that should be updated alongside the code.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The architecture and decomposition are clean, grounded in the codebase, and solve both the Gemini latency bottleneck and Anthropic's `max_tokens` truncation without disturbing `validate*`, the plan cache, `--json` contracts, or the library boundary. Addressing the test/compile items below before implementation will prevent mid-implementation surprises.

## 9. Actionable Feedback

1. **Add `tests/grouping_budget.rs` to Scope/Decomposition/Evaluation (High)**: Update `output_cap_rejection_falls_back_to_one_message_request` (`tests/grouping_budget.rs:426`) to use a file count whose index list exceeds 4096 estimated tokens (~2,000 files), and add a 515-file Anthropic test confirming the grouping `tool_use` request is now sent and succeeds with integer indices.
2. **Use `usize` consistently for `FileRef::Index(usize)` and `PlanError::UnknownIndex(usize)` (Medium)**: Update Section 3 (Prefer) from `Index(u64)` to `Index(usize)` to match `EmptyGroup(usize)` and `changed.len()`, eliminating `u64` -> `usize` conversion edge cases.
3. **Require `Serialize` on `PlanReply` / `ReplyGroup` / `FileRef` (or simplify `anthropic.rs:257`) (Medium)**: Note in Sub-task 2/3 that `extract_tool_use_input` (`/Users/mk/Code/gcm--feat-clo-801-group-index/src/provider/anthropic.rs:258`) calls `serde_json::to_string` on the directly deserialized value, so the wire structs must derive `Serialize` (or `extract_tool_use_input` should serialize `input` directly). Also clarify how Row 14 exercises `Anthropic::generate_plan` end-to-end (e.g., via `TcpListener` + `GCM_ANTHROPIC_BASE_URL`).
4. **Clarify test updates in AC6 and Decomposition (Medium)**: Explicitly note that `plan_error_display_is_distinct` (`/Users/mk/Code/gcm--feat-clo-801-group-index/src/plan.rs:609`) updates from 7 to 8 variants, and `FINGERPRINT_VERSION_GOLDEN` + its expected SHA-256 digest (`/Users/mk/Code/gcm--feat-clo-801-group-index/src/cache.rs:378-388`) update for version 4.
5. **Close Evaluation table gaps (Low)**: Add test cases for non-number JSON types (`true`, `null`, `1.0`) in `files`, a redacted inventory path (`privacy.prepare_grouping` + `resolve` against real `changed`), a renamed file resolved by index (`old_path` ignored), and a full-binary integer-index plan test in `scripts/acceptance.sh` or `tests/grouping_budget.rs`. Note `Repo::at` + tempdir + stub `Provider` for Row 16 in `src/main.rs`.
