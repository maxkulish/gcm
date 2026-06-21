# Spec Review: clo-489

**Reviewer**: Gemini (via lok spec-review)
**Reviewed**: 2026-06-21
**Pipeline**: lok spec-review (synthesis + write_reviews failed on the known NUL-byte bug; this file captures the substantive Gemini output - verdict + actionable feedback - recovered from the run log)

---

## 8. Verdict

**NEEDS_REVISION**

The specification is thorough and well-structured, but has several gaps that could cause implementation issues:

1. Missing concrete values for OpenAI/Gemini diff budgets (AC-8)
2. Missing test coverage for provider alias (`gemini` -> `google`), empty `--model`, and Gemini thought-only responses
3. Missing explicit wiring for `cache_model_id()` integration with the existing fingerprint function
4. Ambiguous location for `gemini_schema()`
5. Missing timeout consideration for different provider latencies

## 9. Actionable Feedback

**Priority 1 (Must address before implementation):**

1. Specify concrete OpenAI/Gemini diff budget values (AC-8). If the values are the same as Groq for v1, state that explicitly.
2. Add explicit `cache_model_id()` integration in sub-task 8. The `cache.rs` `const PROVIDER: &str = "groq"` must be removed and `fingerprint()`/`digest_fingerprint()` updated to fold the provider-qualified model id.
3. Clarify `gemini_schema()` location. Recommend `src/plan.rs` alongside the existing `schema()` for consistency and testability.
4. Add test #19: provider alias parsing - `ProviderId::from_str("gemini")` returns `Ok(ProviderId::Google)`.
5. Add test #20: empty/whitespace `--model` handling - should fall through to per-provider env then default.

**Priority 2 (Should address for completeness):**

6. Add test #21: Gemini response with only thought parts -> `EmptyResponse`.
7. Consider timeout per provider. Add a note: `TIMEOUT_SECS` is shared across providers in v1; may become per-provider in a future slice if latency characteristics differ.
8. Add edge case: whitespace-only `GCM_PROVIDER` should be treated as unset (use default) for resilience.
9. Document the Gemini diff budget value (Gemini's context window is much larger than Groq/OpenAI).

**Priority 3 (Nice to have for maintainability):**

10. Verify `From<ProviderError>` preserves the message format: `GcmError::Provider(e)` displays as the provider-qualified kind message.
11. Add a note about `--model` validation strategy: no client-side model-ID validation in v1; invalid model IDs are rejected by the provider API with actionable errors.

## 7. Blind Spots (carried)

9. Per-provider reasoning suppression is keyed on model-name substrings (`qwen`, `gpt-oss`); new models need a code change. Acceptable for v1, worth noting.
10. Diff budget for Gemini: `gemini-3.1-flash-lite` has a ~1M-token window, much larger than Groq/OpenAI - should it have a larger budget, or are Groq defaults fine?
