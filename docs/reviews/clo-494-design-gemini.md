# Design Review: CLO-494 — Gemini

**Model:** gemini-3.5-flash
**Date:** 2026-06-22
**Verdict:** approve_with_changes

## Strengths

- **Excellent Architectural Consistency:** The proposed `Anthropic` provider integrates perfectly with the existing `Provider` trait and error taxonomy, maintaining consistency with implementations like `Gemini` and `OpenAi`.
- **Robust Response Parsing & Reasoning Suppression:** The parser design for `generate_plan` and `generate_message` is exceptionally robust, proactively parsing native Anthropic message formats, ignoring native `thinking` blocks, and leveraging the fallback `strip_think` backstop.
- **Smart Error Alignment:** The existing `http::bad_request_detail` helper automatically parses Anthropic's structured JSON error objects (specifically the nested `error.message` key) without any modification.
- **Clean & Retrocompatible HTTP Extensibility:** Adding `extra_headers` to `HttpRequest` allows Anthropic to pass its required `anthropic-version` header cleanly without altering the standard, byte-identical requests sent by other providers.
- **Comprehensive Test Coverage Plan:** The test plan details exhaustive unit and integration testing scenarios, including critical edge cases.

## Concerns

1. **`max_tokens` Truncation Handling Gaps (P2):** The `extract_tool_use_input` parser logic in Section 3c does not explicitly check for `stop_reason: "max_tokens"`. If the token budget is exhausted, the API may return an incomplete/malformed tool call. Explicitly checking for `max_tokens` early allows raising a descriptive error rather than letting a generic JSON deserialization error bubble up.

2. **Struct Modification Fan-out (P3):** Modifying the fields of `HttpRequest` directly forces changes in all other provider modules to include `extra_headers: Vec::new()`. This increases boilerplate for any future providers added.

3. **Untestable HTTP Unit Test (P3):** Test 18 mentions verifying that `send_once` sends the extra headers using mocks or inspection. However, `send_once` instantiates a real `ureq::Agent` and performs direct network calls, making it untestable via unit mocks under the current codebase architecture.

## Suggestions

1. **[refinement] Explicitly check for `max_tokens` in response extraction:**
   In `extract_tool_use_input`, intercept any response with `stop_reason == "max_tokens"` and immediately return a custom `ProviderError` with `ErrorKind::Deserialize("Anthropic response truncated (stop_reason: max_tokens); the diff may be too large")`. This matches Gemini's elegant truncation handling.

2. **[refinement] Introduce a default constructor or builder pattern for `HttpRequest`:**
   Implement a builder or constructor for `HttpRequest` that defaults `extra_headers` to `Vec::new()`, along with a `.with_headers(Vec<(&'static str, String)>)` builder method. This prevents having to modify existing provider files and simplifies the addition of future backends.

3. **[additive] Attempt direct deserialization of `tool_use` JSON values first:**
   In `extract_tool_use_input`, the `input` field of a `tool_use` content block is already parsed as a validated `serde_json::Value`. Rather than immediately serializing it back to a string for `plan::parse_defensive`, first attempt direct deserialization using `serde_json::from_value::<Plan>(input)`. Only fall back to `parse_defensive` (via serialization) if direct deserialization fails.

4. **[refinement] Refine the strategy for Test 18:**
   Update the test plan to clarify that Test 18 will be accomplished by either refactoring the request/header formatting logic into a pure, unit-testable helper or relying purely on manual verification/integration testing using a local proxy server.