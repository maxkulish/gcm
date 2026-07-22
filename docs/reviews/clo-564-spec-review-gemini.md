# Spec Review: clo-564

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-07-22
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and highly accurate. It perfectly aligns with the Linear task description (CLO-564). It accurately captures the gap in the current implementation—where Vertex AI discovery short-circuits to static Gemini 3.1 models while Google AI Studio has no such limitation—and documents the correct Vertex API endpoint (`GET https://aiplatform.googleapis.com/v1beta1/publishers/google/models?pageSize=200`) and authentication requirements verified through live testing.

## 2. Acceptance Criteria Review
**Strong**: 
* **AC1 & AC2**: Ensure a clean separation of concerns. `models.rs` is kept free of subprocess calls, and token resolution stays in `vertex.rs` (no network or subprocess during offline fallbacks).
* **AC3 & AC4**: Correctly address refreshing the offline fallbacks and built-in defaults to the 3.5/3.6 generations while preserving the fallback-contains-default test invariant.
* **AC6**: Protects backwards compatibility for existing whitelists without a configuration schema version bump.

**Gaps**:
* **JSON Structure Mismatch**: While the spec mentions a new `parse_models` arm for `publisherModels` in the decomposition section, there is a gap in the Acceptance Criteria (AC5) which should explicitly require robust parsing and de-prefixing of the `"publisherModels"` key instead of `"models"`.
* **Impact of Signature Change**: Adding the `project` parameter to `fetch_supported_models` and `fetch_supported_models_with` is necessary, but the criteria does not mention updating all the existing test suites (e.g. OpenAI, Anthropic, Google, Ollama) that use `fetch_supported_models_with` and will be broken by this signature change.

## 3. Constraints Check
**Aligned**:
* Keeping `models.rs` free of subprocesses matches the project's architecture.
* Utilizing `GCM_VERTEX_BASE_URL` as a test seam preserves the hermetic testing constraint.
* Reusing the existing exclude-list as a capability filter maintains consistent filtering policies.

**Concerns**:
* **Base URL Resolution Isolation**: Currently, in `src/provider/models.rs::resolved_base_url_with`, `ProviderId::Vertex` is grouped under `ProviderId::Google`, mapping it to the Gemini API host (`https://generativelanguage.googleapis.com`) and looking up Gemini-specific environment variables. Leaving this unchanged will break Vertex discovery completely. The Vertex provider needs its own branch to resolve to `"https://aiplatform.googleapis.com"` and use `GCM_VERTEX_BASE_URL`.

## 4. Decomposition Quality
**Well-scoped**:
* All sub-tasks are highly granular, logical, and estimated to take under 2 hours.
* The dependency ordering (`1 -> 2 -> {3, 4}`) is solid.

**Issues**:
* **Missing Test Refactoring Task**: Task 4 (Tests) should explicitly include the work of updating all existing mock and transport tests in `src/provider/models.rs` to pass `None` for the newly added `project` parameter.
* **Base URL Isolation Task**: Under Task 2 (Discovery arm), the spec mentions building the `HttpGet` with `GCM_VERTEX_BASE_URL` but forgets to explicitly state that `resolved_base_url_with` must be refactored to separate `ProviderId::Vertex` from `ProviderId::Google`.

## 5. Evaluation Coverage
**Covered**:
* Serves as an excellent blueprint, covering mock `TcpListener` transport testing (asserting Bearer authentication and project headers), parser tests, static catalog checks, and default-model invariants.
* Leverages `GCM_VERTEX_BASE_URL` to enable 100% hermetic tests with no real network or gcloud subprocesses.

**Gaps**:
* Lacks a test scenario for parsing a Vertex response that contains a missing or empty `"publisherModels"` key to ensure the parser degrades gracefully to static fallbacks instead of panicking on deserialization.

## 6. Codebase Alignment
**Violations**:
* No structural violations are introduced. The specification is fully synchronous, respects the fallback-safety contracts, and aligns with the existing provider abstraction.

**Alignment**:
* The proposed implementation makes clean use of the injectable `fetch_supported_models_with` transport seam introduced in CLO-547.
* Reusing `resolve_access_token` and `probe_adc` (currently defined as stubs in `src/provider/vertex.rs`) perfectly leverages pre-existing codebase components.

## 7. Blind Spots
* **Base URL Mismatch**: Unchecked grouping of `ProviderId::Vertex` and `ProviderId::Google` in `resolved_base_url_with` will direct Vertex discovery to the wrong domain and incorrect env vars.
* **Test Compilation Failures**: Adding `project` to `fetch_supported_models` breaks all existing mock calls in `src/provider/models.rs`.

## 8. Verdict
APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback
1. **Isolate Vertex Base URL Resolution (Critical)**: Refactor `src/provider/models.rs::resolved_base_url_with` to split `ProviderId::Vertex` from `ProviderId::Google`. Assign `&["GCM_VERTEX_BASE_URL"]` as environment overrides and default to `"https://aiplatform.googleapis.com"` for Vertex.
2. **Account for Signature Change in Tests (High)**: Add a subtask under Decomposition Step 4 to update approximately 10-15 existing mock test cases in `src/provider/models.rs` (e.g. OpenAI, Ollama, Anthropic) to pass `None` for the project parameter in `fetch_supported_models_with`.
3. **Robust Vertex Parser Tests (Medium)**: Write a unit test specifically asserting that `parse_models` gracefully handles a missing `"publisherModels"` key, empty lists, or malformed values by returning an empty vector (triggering the static catalog fallback).
