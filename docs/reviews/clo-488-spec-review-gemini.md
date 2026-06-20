# Spec Review: clo-488

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-20
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement in Section 1 is exceptionally clear, self-contained, and comprehensive. It accurately identifies that the merged provider layer (`src/groq.rs`) currently collapses all HTTP status failures into a coarse `GroqError::Http(u16)` with zero retry or backoff support. 

It perfectly aligns with the Linear task description (CLO-488) and explicitly references the relevant files, functions, and lines in the codebase (e.g., `src/groq.rs:39-47`, `send_chat`, `generate_plan`). 

The document also correctly cites the driver decisions in ADR-001—particularly Decision 2 (retaining a blocking client with no async runtime) and Decision 3 (typed errors and retries)—ensuring the problem context is fully grounded in the project's architectural bounds. No unstated assumptions or contradictions were identified.

---

## 2. Acceptance Criteria Review

**Strong**:
- **AC-1, AC-4, AC-6, AC-7**: The retry behaviors are divided elegantly between pure retry policy assertions (e.g., retryable status classifications, backoff schedules) and stateful retry loops tested via an injected sleeper closure. This makes testing deterministic and extremely robust without requiring actual network requests or thread sleeps in unit tests.
- **AC-2, AC-3**: The requirement to verify exactly one request for non-retryable paths (400 and 401/403) on the single-commit (`--all`) path directly addresses the need to fail fast without retry loops.
- **AC-9**: The layered strategy for defensive parsing (fence stripping, direct parsing, balanced brace extraction, and recursive key searching) is highly actionable and easy to verify.

**Gaps**:
- **Display Representation**: While AC-5 asserts that six core variants must produce six different, non-empty messages, it does not explicitly mention that `PlanError::Parse` (added in AC-9) must also implement a descriptive `Display` message.
- **Case-Insensitive Headers**: There is a minor gap regarding HTTP response header casing. While `Retry-After` is the standard capitalization, HTTP headers are case-insensitive. The ACs do not explicitly mandate checking header lookups in a case-insensitive manner.

---

## 3. Constraints Check

**Aligned**:
- **No Async Runtime / No New Dependencies**: The "Must" constraints perfectly enforce the blocking `ureq` client pattern and forbid introducing `tokio`, `rand`, or logging frameworks (`tracing`/`log`), matching the fast cold-start and binary size goals of ADR-001.
- **Pure Functions for Policy**: Using pure functions (`classify_status`, `is_retryable`, `backoff_delay`) guarantees testability and keeps side effects isolated.
- **Index Preservation**: Explicitly confirming that all retries occur before index mutation ensures transactional safety (FR-47).

**Concerns**:
- **Status Classification Terminology**: In the constraints of Section 3, `Auth` is named as `Auth(u16)`. In Table 3b, it is written as `Auth(code)`. In the description of GcmError, it is routed as `Auth(_)`. This should be normalized to `Auth(u16)` across all locations for absolute clarity in code generation.

---

## 4. Decomposition Quality

**Well-scoped**:
- The 6-step decomposition is logical, incremental, and highly structured.
- **Step 1 (Debug helper)** is a leaf module that enables immediate logging in subsequent tasks.
- **Step 2 (Taxonomy)** and **Step 3 (Retry)** flow naturally into each other.
- **Step 4 (Defensive parsing)** is isolated in `src/plan.rs` and can be implemented in parallel.

**Issues**:
- **Sub-task 4 Error Mapping**: Sub-task 4 mentions that `generate_plan` calls `parse_defensive`, which returns a `Result<Plan, PlanError>`. However, `generate_plan` returns `Result<Plan, GroqError>`. It should explicitly specify that `PlanError` must be mapped to `GroqError::Deserialize(msg)` within `generate_plan` to avoid compilation/signature mismatches.

---

## 5. Evaluation Coverage

**Covered**:
- The evaluation matrix (20 test cases) is exemplary.
- It covers edge cases like a `Retry-After` header parsing of `0`, absurdly large values, HTTP dates, and integer overflow guards in the backoff calculation (`.min(16)` clamping).

**Gaps**:
- **Multiple Markdown Code Fences**: There is no specific test case for a response containing multiple markdown blocks or mixed prose before the markdown block. Adding a specific unit test for `parse_defensive` with multiple JSON blocks will guarantee robust extraction.

---

## 6. Codebase Alignment

**Violations**:
- No structural or architectural violations were identified.

**Alignment**:
- **Error Routing**: The routing in `main.rs build_plan` (`MissingKey | Auth(_)` as `Fatal`, others as `Fallback`) perfectly matches the existing single-commit fallback policy, ensuring that API key authentication issues fail fast while other issues (like transient server errors or formatting errors) attempt to recover gracefully.
- **Standard Library Sleep**: Utilizing `std::thread::sleep` directly aligns with the synchronous and lightweight design of the current command-line interface.

---

## 7. Blind Spots

1. **Defensive Body Read (HTML Floods)**:
   If a server returns a non-2xx error (e.g., a Cloudflare 502/504 error page), the response body could be a massive HTML document. If we read the entire response body into a string without constraints, it could cause high memory overhead or read-latency hangs. We must limit the body read using `.take()` on the reader (e.g., capped at 4096 bytes).
2. **Groq Error JSON Extraction**:
   If a bad request (400) occurs, Groq returns structured JSON (e.g., `{"error": {"message": "..."}}"}`). Simply dumping the raw response string as `detail` will look messy in CLI output. The implementation should attempt to parse the body as JSON and extract `error.message` first, falling back to a truncated raw string if parsing fails.
3. **Injected Sleeper Signature**:
   The retry loop pseudocode shows `sleep: impl Fn(Duration)`. In unit tests, a mock sleeper needs to mutate local state (e.g., a captured `Vec<Duration>` of recorded sleep intervals). Therefore, the signature of the injected sleeper should be `impl FnMut(Duration)` to avoid requiring interior mutability boilerplate (like `RefCell` or `Mutex`) in tests.

---

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

---

## 9. Actionable Feedback

1. **Defensive Body-Read Capping (High Priority)**:
   In `send_chat_once` (or status inspection), do not read the entire response body unbounded on non-2xx status codes. Use a reader limit of 4096 bytes:
   ```rust
   let mut body_str = String::new();
   response.body_mut().as_reader().take(4096).read_to_string(&mut body_str)?;
   ```
2. **Groq Error JSON Extraction (Medium Priority)**:
   Implement structured message extraction for the `detail` parameter on `BadRequest`. If the body is valid JSON and contains `error.message`, extract it; otherwise, truncate the raw string to 200 characters to keep CLI errors clean and actionable.
3. **Use `FnMut` for the Injected Sleeper (Medium Priority)**:
   Update the retry loop (`retry_with`) signature to use `FnMut` instead of `Fn`:
   ```rust
   pub fn retry_with<T>(
       cfg: &RetryConfig,
       mut sleep: impl FnMut(Duration),
       mut op: impl FnMut() -> Result<T, GroqError>,
   ) -> Result<T, GroqError>
   ```
4. **Header Lookup Case-Insensitivity (Medium Priority)**:
   Explicitly specify that looking up `"Retry-After"` on the response headers must be case-insensitive (e.g., checking both `"Retry-After"` and `"retry-after"` or utilizing `ureq`'s native case-insensitive header getter).
5. **Add `PlanError::Parse` Display Implementation (Low Priority)**:
   Ensure `fmt::Display` for `PlanError` is updated to handle the new `Parse(String)` variant:
   ```rust
   PlanError::Parse(msg) => write!(f, "plan parse error: {msg}")
   ```
6. **Precedence Hierarchy for `parse_defensive` (Low Priority)**:
   Document the search precedence for layer 4 in `parse_defensive`:
   1. Direct search for `groups` array at the top level.
   2. Search within known wrapper keys (`commit_plan`, `plan`, `result`, `data`, `response`).
   3. Depth-first recursive search for any key named `"groups"` holding an array.
