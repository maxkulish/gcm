# Spec Review: clo-495

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-22
**Pipeline**: lok spec-review

---

Now I have comprehensive context. Let me provide a thorough specification review.

---

## Specification Review: CLO-495 Ollama Local Provider

---

## 1. Problem Statement Assessment

**Clear and Well-Defined**: The problem statement correctly identifies that `gcm` currently lacks a zero-egress option for privacy-sensitive repositories. The specification accurately references FR-56 and ADR-001 Decision 8, establishing the architectural mandate for a local Ollama provider.

**Matches Linear Task Description**: The spec aligns precisely with the Linear task: "Add a local Ollama backend (native /api/chat with format=schema, no API key, default endpoint http://localhost:11434 overridable via OLLAMA_HOST) so commits generate with zero egress, covering FR-56."

**Key Facts from Codebase Probe Are Excellent**: The spec thoroughly probes the existing codebase and documents:
- The `Provider` trait contract (`src/provider/mod.rs:27`)
- The selection registry pattern (`ProviderId` enum, `select()` match)
- The `HttpRequest.auth` mandatory tuple and the proposed `Option<>` change
- The shared transport and error handling patterns

**Minor Clarification Needed**: The spec states "The architecture is already decided in ADR-001 Decision 8" - this is correct and well-documented. However, the spec references `gcml` alias which is explicitly out of scope (CLO-497). This is correctly noted as "not here" but could be clearer that no code changes should reference `gcml` in this task.

**Verdict**: ✅ Problem statement is clear, complete, and accurate.

---

## 2. Acceptance Criteria Review

### Strong Criteria:
- **AC-1 (zero-egress local commit)**: Measurable - "only the configured local endpoint contacted, no API key set, no third-party host request"
- **AC-2 (unreachable → actionable error)**: Measurable - exit code, index state, error message content all specified
- **AC-3 (provider selectable, key-free)**: Testable via `--provider=ollama` and `GCM_PROVIDER=ollama`
- **AC-4 (model selection)**: Clear precedence chain with default `gemma4:e4b-mlx`
- **AC-5 (endpoint config)**: Clear precedence chain with `OLLAMA_HOST` normalization
- **AC-7 (missing-model → actionable error)**: Specific error message format with `ollama pull` guidance

### Gaps:

**AC-6 Gap - Response Parsing Edge Cases**: The spec says "parse from `message.content` defensively" but doesn't specify what happens when:
1. `message.content` is present but contains invalid JSON (covered by defensive parsing reference)
2. `message` object is missing entirely from response (what error?)
3. Response is valid JSON but has unexpected shape (e.g., `{"model": "...", "created_at": ...}` without `message`)

**Recommendation**: Add explicit handling for missing `message` key - the spec should specify mapping to `ErrorKind::Deserialize` with a clear message.

**AC-1 Gap - "Only Local Endpoint Contacted"**: How is this verified? The acceptance tests use a mock server, but there's no explicit assertion that no external network calls occur. Consider:
- Adding a note that unit tests use dependency injection and mock the HTTP layer, ensuring no real network calls
- Or documenting that this is verified by manual testing / code review of the endpoint URL construction

**Missing AC for OLLAMA_HOST Empty/Whitespace**: The spec mentions "empty/whitespace → treated as unset" in edge cases, but this should be an explicit AC for consistency with how empty env vars are treated in other providers.

**Verdict**: ✅ Criteria are generally strong with minor gaps that can be addressed.

---

## 3. Constraints Check

### Aligned with Codebase Patterns:

**Provider Trait Implementation**: The constraint to implement `Provider` in `src/provider/ollama.rs` follows the exact pattern of `groq.rs`, `openai.rs`, and `gemini.rs`.

**Native `/api/chat` Endpoint**: Correctly avoids the OpenAI-compatible path per ADR-001 Appendix A.

**No API Key**: Correctly identifies that Ollama needs no auth, and proposes making `HttpRequest.auth` optional.

**Default Model**: `gemma4:e4b-mlx` aligns with the owner's local Apple Silicon context.

**Error Handling**: Using existing `ErrorKind` taxonomy (Transport, Http(404), Deserialize, EmptyResponse) aligns perfectly with CLO-488.

### Concerns:

**Concern 1 - `HttpRequest.auth` Optional Change Scope**: The spec says "(it should not)" regarding ripple effects beyond the 4 provider builders + `send_once`. However, I should verify this claim against the actual http.rs:

From the code, `HttpRequest.auth` is a `(&'static str, String)` tuple used only in `send_once`:
```rust
pub auth: (&'static str, String),
// ...
.header(req.auth.0, req.auth.1.as_str())
```

Making this `Option<(&'static str, String)>` is indeed a small, contained change. The spec's assessment is accurate.

**Concern 2 - Error Remapping Inside ollama.rs**: The spec correctly requires error remapping to happen *inside* `ollama.rs` without widening the shared `ErrorKind` taxonomy. However, there's a subtle issue:

The spec says "Transport(msg) → enriched message (endpoint + 'is Ollama running?...')". But looking at `ErrorKind::Transport(String)`:
```rust
Transport(String)
```

This is a `String` field, so the enriched message can be constructed. However, the spec should clarify that this enrichment happens at the `ollama.rs` level by inspecting the `ureq::Error` and constructing a new `ProviderError` with a more specific message, not by modifying the `ErrorKind` enum.

**Concern 3 - Model Name Validation**: The spec says "no client-side model-id validation in v1" (consistent with CLO-489). However, for Ollama, HTTP 404 is a reliable signal for "model not pulled" - this is specific to Ollama's API. The spec correctly handles this in AC-7.

**Verdict**: ✅ Constraints are well-aligned with the codebase; minor clarifications noted.

---

## 4. Decomposition Quality

### Well-Scoped Sub-Tasks:

**Task 1 - Registry wiring**: Correctly identified changes to `ProviderId` enum, `default_model`, `model_env_vars`, `select()` arm, and unknown-provider error list. Estimated scope is appropriate (~2 hours).

**Task 2 - Optional auth in shared transport**: Correctly scoped to `http.rs` and the 4 provider files. This is a prerequisite for Task 3.

**Task 3 - Ollama backend core**: Correctly scoped - own file, implements Provider trait, own payload builders and response parser.

**Task 4 - Actionable errors**: Scoped to error remapping inside `ollama.rs` - correct placement.

**Task 5 - CLI surface**: Correctly identifies `cli.rs` updates for `--provider` doc comment and egress disclosure.

**Task 6 - Acceptance + docs**: Extends mock server and adds AC-O* cases.

### Issues:

**Issue 1 - Missing Dependency Identification**: Task 3 depends on Task 2, but Task 1 is independent. However, the spec shows Task 3 also needing the `plan::schema()` helper, which already exists. The dependency chain is correctly stated.

**Issue 2 - Sub-task Size Estimates**: The spec estimates "~2 hours" per sub-task implicitly. Let me assess:
- Task 1 (Registry): ~1-2 hours - accurate
- Task 2 (Optional auth): ~1-2 hours - accurate (mechanical change)
- Task 3 (Ollama core): ~3-4 hours - potentially under-estimated given the payload builders, response parser, endpoint normalization
- Task 4 (Actionable errors): ~1 hour - accurate
- Task 5 (CLI): ~30 min - accurate
- Task 6 (Acceptance): ~2-3 hours - potentially under-estimated for mock server route additions

**Issue 3 - Missing Sub-task for `ProviderId::parse("ollama")` Case-Insensitivity**: The spec mentions accepting `OLLAMA` as an alias but doesn't explicitly call this out as a test case in Task 1. Looking at existing code:
```rust
ProviderId::parse("ollama") // Should this also accept "OLLAMA"?
```

The existing `ValueEnum` derives `#[value(rename_all = "lower")]` which would normalize. This is probably fine but worth explicit testing.

**Verdict**: ✅ Decomposition is solid with minor estimation concerns.

---

## 5. Evaluation Coverage

### Covered Criteria:

Tests 1-14 in the evaluation table map well to AC-1 through AC-7:
- Test 1-5: Provider registry and selection
- Test 6-7: Payload shapes (stream:false, format, temperature)
- Test 8-9: Response parsing (thinking ignored, empty → EmptyResponse)
- Test 10: Endpoint precedence and normalization
- Test 11: cache_model_id format
- Test 12-13: Error remapping (Transport, 404)

AC-O* acceptance tests cover integration scenarios against mock server.

### Missing Test Scenarios:

**Gap 1 - No Test for "No Authorization Header Sent"**: The spec emphasizes that Ollama sends no auth header, but there's no explicit test verifying the header is absent. The acceptance test should capture HTTP headers and assert `Authorization` is not present.

**Gap 2 - No Test for `message.thinking` Present in Response**: Test 8 checks `thinking` is ignored, but doesn't verify a response *with* a `thinking` field still produces valid output. Add a test case where:
```json
{"message":{"content":"{...}","thinking":"reasoning here"}}
```
and verify the parsed plan is correct.

**Gap 3 - No Test for Malformed OLLAMA_HOST**: The spec mentions "scheme-less normalization" but doesn't test malformed cases like:
- `OLLAMA_HOST=http://localhost:11434` (already has scheme) - should work as-is
- `OLLAMA_HOST=://broken` - should fail or fall back to default?

**Gap 4 - No Test for Concurrent Request Behavior**: Not critical for this spec, but the spec should note that Ollama (like other providers) makes sequential calls (plan then message), not concurrent.

**Verdict**: ✅ Good coverage with a few gaps noted.

---

## 6. Codebase Alignment

### Pattern Adherence:

**Provider Trait Implementation**: The spec correctly mirrors the pattern from `gemini.rs`:
- Own parser (`extract_content` or similar)
- Own payload builders (`build_plan_payload`, `build_message_payload`)
- `Provider` impl with `generate_plan`, `generate_message`, `cache_model_id`, `diff_budget`

**Error Handling**: Uses existing `ErrorKind` variants correctly:
- `Transport(String)` for connection failures
- `Http(404)` for missing model → remapped with actionable message
- `EmptyResponse` for empty `message.content`
- `Deserialize(String)` for parse failures

**Module Registration**: Correctly identifies adding `mod ollama;` and `Ollama` variant to `ProviderId`.

### Pattern Violations:

**Violation 1 - `HttpRequest.auth` Required Pattern**: The existing pattern has `auth` as a required tuple. Making it optional is a breaking change to the pattern. The spec correctly identifies this and proposes the `Option<>` change. However:

**Recommendation**: Add a compile-time assertion that ensures all providers pass either `Some(...)` or `None` explicitly, not forgetting to update auth when adding a new provider.

**Minor Deviation - No `auth_env_var` for Ollama**: The existing `HttpRequest` struct has `auth_env_var` for error messages. For Ollama, there is no auth, but the field is still required. The spec should address this:
- Option A: Set `auth_env_var` to `""` or a placeholder
- Option B: Make `auth_env_var` optional alongside `auth`

Looking at the code:
```rust
pub(super) struct HttpRequest<'a> {
    pub provider: &'static str,
    pub auth_env_var: &'static str, // Used in Auth error messages
    pub endpoint: String,
    pub auth: (&'static str, String),
    pub payload: &'a Value,
}
```

Since Ollama can't produce `Auth` errors (no auth), the field is moot. But the struct requires it. The spec should clarify this - likely set `auth_env_var: ""` since it's unreachable code.

### Verification Against Existing Patterns:

**`gemini.rs` as Closest Template**: The spec correctly identifies `gemini.rs` as the closest template because:
- Own endpoint shape (not `/chat/completions`)
- Own response parser (`extract_text`)
- Own payload builders

**`diff_budget()` Pattern**: The spec says "Prefer `DiffBudget::standard()` for `diff_budget()`" which matches the Groq/Gemini pattern.

**`cache_model_id()` Pattern**: `"ollama:<model>"` format matches the existing `"groq:..."`, `"google:..."`, `"openai:..."` pattern.

**Verdict**: ✅ Strong alignment with established patterns; one minor struct field issue noted.

---

## 7. Blind Spots

### What the Specification Misses:

**Blind Spot 1 - Timeout Configuration for Ollama**: The spec doesn't address timeouts. Local LLMs (especially on Apple Silicon MLX) can be slower than cloud APIs. The existing `GCM_HTTP_TIMEOUT_SECS` env var applies to all providers. Should Ollama have:
- Same 60s default?
- A longer default for local inference?
- A way to configure per-provider timeouts?

**Recommendation**: Document that the shared `GCM_HTTP_TIMEOUT_SECS` applies, and that local inference may need longer timeouts for complex diffs.

**Blind Spot 2 - Ollama Version Compatibility**: The spec references ADR-001 Appendix A verification from 2026-06-19. The Ollama API (`/api/chat` with `format` field) may evolve. The spec should note:
- Minimum Ollama version tested
- Fallback behavior if `format` is rejected (400 response)
- Whether structured output fidelity varies by pulled model

**Blind Spot 3 - `OLLAMA_HOST` Trailing Slash**: The spec says "trim trailing-slash" but the edge case of `OLLAMA_HOST=http://localhost:11434/` vs `OLLAMA_HOST=http://localhost:11434` should be explicitly tested. Looking at existing code:
```rust
self.base_url().trim_end_matches('/')
```
This is the correct pattern; the spec correctly identifies it.

**Blind Spot 4 - Streaming is Omitted but Not Disabled**: The spec says "omit streaming" with `"stream": false`. However, Ollama's `/api/chat` defaults to streaming if `stream` is not explicitly set to `false`. The spec correctly requires setting `"stream": false`, but this is critical - missing this would cause NDJSON response parsing failures.

**Blind Spot 5 - `OLLAMA_HOST` Port Default**: The spec shows normalization `127.0.0.1:11434` → `http://127.0.0.1:11434`. But what about:
- `localhost` (no port) → should use default 11434
- `my-server.local` (no port) → should use default 11434

The spec should clarify that scheme-less values without a port get the default port appended.

**Blind Spot 6 - Error Message for Connection Refused vs Daemon Not Running**: The spec says "is Ollama running?" but doesn't distinguish between:
- Connection refused (daemon not running)
- DNS failure (hostname unreachable)
- Timeout (daemon slow/frozen)

The error message should be actionable for each case. Currently `Transport(String)` contains the underlying error, which is fine.

**Blind Spot 7 - Integration with Existing Acceptance Tests**: The spec proposes new AC-O* tests, but doesn't clarify if these run alongside existing tests or require Ollama-specific setup. Since CI has no Ollama daemon, the mock server approach is correct, but this should be explicit.

**Blind Spot 8 - `:cloud` Model Documentation**: The spec correctly notes that `:cloud` models egress to Ollama Cloud and are **not** zero-egress. However, the spec should clarify:
- Is this documented in CLI help? In README?
- What error/warning, if any, should be shown when a user selects a `:cloud` model?

**Blind Spot 9 - Model Pull Verification**: AC-7 says "model not pulled" error. But the spec doesn't address:
- How to distinguish HTTP 404 (model not found) from other 404s
- Whether Ollama returns a specific error body for "model not found"

Looking at Ollama API, the error response is typically:
```json
{"error": "model 'gemma4:e4b-mlx' not found, try pulling it first"}
```

The spec should note parsing this error body for a more actionable message.

**Verdict**: ⚠️ Several blind spots identified, mostly around edge cases and integration details.

---

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is thorough, well-aligned with the codebase architecture, and correctly identifies the changes needed to add Ollama as a zero-egress provider. The decomposition is sound, acceptance criteria are measurable, and the evaluation approach is realistic.

The suggestions below are improvements, not blockers.

---

## 9. Actionable Feedback

### Priority 1 - Must Address Before Implementation:

1. **Clarify `HttpRequest.auth_env_var` handling**: For Ollama, set `auth_env_var: ""` since no auth is required. Update `send_once` to only use `auth_env_var` when `auth.is_some()`.

2. **Add explicit test for "No Authorization Header"**: Add an acceptance test that captures HTTP headers and asserts `Authorization` header is absent when `--provider=ollama`.

3. **Document Ollama API response shape for missing model**: Specify that the 404 response body contains `{"error": "model '...' not found..."}` and parse this for the actionable message in AC-7.

### Priority 2 - Should Address:

4. **Add `OLLAMA_HOST` port-default behavior**: When normalizing a scheme-less value without a port (e.g., `localhost`), append `:11434` to get `http://localhost:11434`.

5. **Document timeout expectations**: Add a note that local inference may require longer timeouts and that `GCM_HTTP_TIMEOUT_SECS` applies to Ollama as well.

6. **Add `:cloud` model documentation**: Add a note in CLI help or README about `:cloud` models not being zero-egress.

### Priority 3 - Minor Improvements:

7. **Add missing response shape handling**: Specify error for response missing `message` key entirely → `ErrorKind::Deserialize("Ollama response missing 'message' key")`.

8. **Clarify `OLLAMA_HOST` empty/whitespace handling**: Add explicit test case for `OLLAMA_HOST=""` and `OLLAMA_HOST="   "`.

9. **Update evaluation table**: Add test cases for:
   - Response with `thinking` field present
   - `OLLAMA_HOST` with trailing slash
   - `OLLAMA_HOST` with port vs without port

10. **Estimate Task 3 more conservatively**: Ollama backend core includes endpoint normalization, response parsing, and error mapping - estimate 3-4 hours rather than ~2.
