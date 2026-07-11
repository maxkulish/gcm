# Spec Review: clo-545

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-07-10
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and accurate. It perfectly matches the Linear task description (CLO-545), precisely identifies all touched components (mod.rs, models.rs, openai.rs, cli.rs, diff.rs, config.rs, and README.md), and correctly details the target models (`gpt-5.6-luna` default, `gpt-5.6-terra` fallback).

## 2. Acceptance Criteria Review
**Strong**:
* Criteria **AC1–AC3** are highly specific, verifiable, and map directly to compile-time or unit-test assertions.
* **AC4** provides an exact, reproducible `rg` search pattern to prevent string leakage.
* **AC5** pinpoints the exact line ranges in `README.md` to update.
* **AC7** establishes a realistic smoke-test baseline using the actual API.

**Gaps**:
* **Transition Validation Caveat**: A user with an existing `gcm.toml` (v2 configuration) will have `providers.openai.models` whitelisted to the legacy models (`gpt-5.4-mini`, `gpt-4o-mini`, etc.). Upon upgrade, the CLI will resolve the default model to `gpt-5.6-luna` but immediately crash with a validation error because `gpt-5.6-luna` is not in their stale config whitelist. There is no criterion specifying how to smooth this transition or warn the user.

## 3. Constraints Check
**Aligned**:
* Removing the o-series reasoning-override path aligns with the owner's goal to streamline OpenAI integration.
* Preserving the numeric `diff_budget` limits (256,000 bytes / standard per-file) while updating only the comment matches local performance limits.
* Pinned scope for the regex sweep (`src/` + `README.md`) successfully protects historical logs and design documentation under `docs/` from unnecessary churn.

**Concerns**:
* No concerns found. The constraints are rigorous and respectful of the existing codebase architecture.

## 4. Decomposition Quality
**Well-scoped**:
* **ST1–ST4** are independent, highly focused sub-tasks that can easily be completed in under 2 hours each.
* Dependencies are correctly identified, isolating docs and full gating (ST4) to run last.

**Issues**:
* None. The decomposition is solid.

## 5. Evaluation Coverage
**Covered**:
* The evaluation table covers all functional and non-functional acceptance criteria with concrete verification steps.

**Gaps**:
* **Test Name Mismatch**: Row 1 lists `cargo test default_model` to verify the default model. The actual test in `src/provider/mod.rs` is `provider_defaults_and_tokens`. Running `cargo test default_model` will not match any test.
* **Fallback Content Assertion**: Row 2 lists `cargo test fallback_always_contains_default_model` but this test only asserts that the fallback list *contains* the default model. It does not verify that the list is *exactly* `["gpt-5.6-luna", "gpt-5.6-terra"]`.
* **Missing `keep_chat_model` Verification**: The evaluation table does not explicitly mention running or updating `keep_chat_model_excludes_non_text_for_openai_groq`, which contains assertions on legacy strings (`gpt-5.4-mini`, `gpt-4o`) that will fail AC4.

## 6. Codebase Alignment
**Violations**:
* None. The spec honors the synchronous `Provider` trait contract, the `ProviderError` / `ErrorKind` enum taxonomy, and the standard configuration format.

**Alignment**:
* Deleting `is_reasoning_model` and `system_role` to inline plain-chat behavior (`"system"` role and `temperature`) reduces code complexity and matches the design pattern of other non-reasoning providers.

## 7. Blind Spots
* **Active Whitelist Lockout**: As noted in §2, active users with an existing `gcm.toml` whitelist containing only legacy OpenAI models will be locked out on binary launch once the default switches to `gpt-5.6-luna`. Since `model_is_enabled` suggests running `gcm provider` or `gcm --reconfigure`, the user has a recovery path, but this transition friction is a major blind spot that should be documented in the spec/release notes.

## 8. Verdict
`APPROVE_WITH_SUGGESTIONS`

## 9. Actionable Feedback
1. **Fix Test 1 Runner**: Correct the verification command for Row 1 in the Evaluation Table to `cargo test provider_defaults_and_tokens`.
2. **Add Strict Fallback List Assertion**: Modify Row 2 of the Evaluation Table to explicitly require adding a strict content assertion (e.g., `assert_eq!(static_fallback_models(ProviderId::Openai), vec!["gpt-5.6-luna", "gpt-5.6-terra"])`) inside the fallback tests.
3. **Include `keep_chat_model` in ST1**: Explicitly list the test `keep_chat_model_excludes_non_text_for_openai_groq` in ST1 and the Evaluation Table as a unit test requiring retargeting to ensure the AC4 sweep passes.
4. **Document the Whitelist Transition Caveat**: Add a note in §3 (Constraints) or §7 (Edge cases) indicating that existing users with whitelists populated with legacy OpenAI models will need to run `gcm provider` or `gcm --reconfigure` to update their config whitelists.
5. **Full Function Elimination**: Ensure the spec explicitly mandates deleting `apply_model_params` and `apply_model_params_resolve` entirely in ST2, inlining `temperature` directly into the respective payload builders (`build_plan_payload`, `build_message_payload`, `build_resolve_payload`) to maximize code simplification.
