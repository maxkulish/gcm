# Spec Review: clo-495

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-22
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, self-contained, and highly accurate. It perfectly captures the requirements outlined in the Linear issue (CLO-495) and aligns with the broader privacy epic (FR-48–50). It correctly identifies the lack of a zero-egress local provider in the current codebase and specifies how the new Ollama provider acts as a local privacy anchor. Crucially, the problem statement clarifies that `:cloud` models (e.g. `*:cloud` proxied through Ollama Cloud) are an explicit, documented opt-in and do not count as zero-egress, preserving the integrity of the default offline guarantee.

## 2. Acceptance Criteria Review
**Strong**: AC-2 and AC-7 are exceptional, focusing heavily on error/edge-case paths (unreachable daemon and unpulled models), ensuring that a user running a local model receives clear, actionable guidance. AC-4 and AC-5 establish clear, logical precedence rules for model and endpoint configuration that mirror other providers while respecting Ollama's native environment variable patterns (`OLLAMA_HOST`). AC-6 proactively addresses the structured-output contract (non-streaming, native schema validation) and reasoning hygiene (discarding the separate `message.thinking` field).

**Gaps**: Port defaulting in host normalization requires explicit clarification. If a user sets `OLLAMA_HOST=127.0.0.1` or `OLLAMA_HOST=localhost` (without a port), a simple normalization that only prepends `http://` will result in `http://127.0.0.1` (port 80), which will fail. The AC should explicitly state that a missing port defaults to `11434` to match Ollama's native CLI resolution logic.

## 3. Constraints Check
**Aligned**: The constraints strictly adhere to the codebase's core architecture rules: blocking synchronicity, no async runtime, and zero external CLI subprocess execution. The choice to refactor `HttpRequest.auth` to be optional (`Option`) is excellent, preserving transport truthfulness by not sending fake credentials and correctly confining auth handling to the active provider's implementation.

## 4. Decomposition Quality
**Well-scoped**: The decomposition is outstanding with 6 highly granular, logical, and sequentially structured sub-tasks. Task 2 (optional auth) and Task 3 (core `ollama.rs` implementation) are decoupled, preventing large, monolithic PRs. Dependencies between tasks are accurately mapped.

## 5. Evaluation Coverage
**Covered**: The test table is comprehensive, covering 14 unit test cases and 4 end-to-end integration scenarios (AC-O1 through AC-O4). The integration testing approach is realistic, extending the existing stateful python mock to support Ollama's native `/api/chat` route and mock unpulled model (404) responses.

**Gaps**: No unit test scenario asserts the port-defaulting behavior for scheme-less and port-less `OLLAMA_HOST` configurations.

## 6. Codebase Alignment
**Violations**: None found.

**Alignment**: The specification perfectly follows the synchronous `Provider` trait contract and correctly maps local errors using the existing `ErrorKind` taxonomy. It aligns with defensive parsing and the universal `<think>` strip backstop.

## 7. Blind Spots
- **Ollama Host Port Defaulting**: Normalizing `OLLAMA_HOST` requires careful parsing. Ollama's default daemon runs on port `11434`. Standardizing scheme-less hosts requires appending `:11434` when no port is present, unless the host is a custom HTTPS domain.
- **Error Display Prefixes**: Standard `ErrorKind::BadRequest` and `ErrorKind::Transport` variants display with fixed prefixes that can undermine custom, actionable messages required for AC-2 and AC-7.
- **HttpRequest `auth_env_var` Field**: Since `HttpRequest` has a mandatory `auth_env_var: &'static str` field, the spec should specify what placeholder value `ollama.rs` should pass.

## 8. Verdict
**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback

1. **Implement Port Defaulting in Host Normalization (High Priority)**: Ensure that the host normalization logic in `src/provider/ollama.rs` defaults the port to `11434` when normalizing `OLLAMA_HOST` if no port is present.
   - Example: `OLLAMA_HOST=localhost` → `http://localhost:11434`
   - Example: `OLLAMA_HOST=127.0.0.1:8080` → `http://127.0.0.1:8080`
   - Example: `OLLAMA_HOST=https://custom.domain` → `https://custom.domain` (no port appended)

2. **Utilize `ErrorKind::Config` for Actionable Remapping (Medium Priority)**: In `ollama.rs`, map the remapped unreachable daemon (Transport error) and the 404 missing model (Http 404 error) to `ErrorKind::Config(String)`. Since `ErrorKind::Config` displays its message verbatim without standard prefixes or suffixes, this gives absolute control over the terminal output and keeps the setup instructions pristine.

3. **Define `auth_env_var` for Ollama Requests (Low Priority)**: Clarify that `ollama.rs` should pass `"OLLAMA_HOST"` (or a blank string `""`) as the `auth_env_var` value to `HttpRequest` to satisfy the struct's field requirements without modifying existing fields across other providers.
