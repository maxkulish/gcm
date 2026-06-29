# Spec Review: clo-517

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-29
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is exceptionally clear, complete, and technically accurate. It perfectly aligns with the Linear task description and details the precise root cause (Ollama cloud passthrough models treating the `format` parameter as a no-op). The reproduction evidence provided is concrete and verified.

## 2. Acceptance Criteria Review
**Strong**:
* Explicit criteria to modify `GROUPING_SYSTEM_PROMPT` to include both shape descriptions and a literal example.
* Clear instructions for `recover_groups` behavior to map the near-miss `commits`/`message` alias to the canonical `groups`/`commit_message`.
* Strong emphasis on preserving existing local GGUF models' strict structured parsing via `format` as the primary parse path.

**Gaps**:
* Missing cache invalidation criteria: Changing `GROUPING_SYSTEM_PROMPT` represents a behavioral change in plan generation. The specification must explicitly mandate bumping `FINGERPRINT_VERSION` in `src/cache.rs` from `2` to `3` to guarantee stale cached plans generated under the old prompt contract are discarded and re-analyzed.

## 3. Constraints Check
**Aligned**:
* **Must**: Unchanged `format` field for local GGUF models, trying direct `Plan` parse first, and keeping validator (`validate`) checks intact (e.g., preserving first-message-required rules).
* **Must-not**: Avoiding modifications to the canonical schema or breaking existing defensive parser test suites.
* **Prefer**: Applying normalization logic directly inside/near `recover_groups` keeping parsing flows clean.

**Concerns**:
* The constraint to preserve current caching guarantees is implicitly contradicted if `FINGERPRINT_VERSION` is not bumped.

## 4. Decomposition Quality
**Well-scoped**:
* Sub-task 1 (Prompt update) and Sub-task 2 (Parser normalization) are correctly identified as independent, parallelizable units of work scoped to under 2 hours.
* Sub-task 3 (Tests + docs) is logically sequential to Sub-task 2.

**Issues**:
* Bumping `FINGERPRINT_VERSION` in `src/cache.rs` is omitted from the sub-task list. This should be explicitly added to Sub-task 1 or 2.

## 5. Evaluation Coverage
**Covered**:
* End-to-end `parse_defensive` unit tests covering both single-group and multi-group near-miss `commits` structures.
* Regression checks asserting existing test suites pass.
* Edge cases (e.g., dual keys, validation failures, mixed response arrays) are proactively addressed.

**Gaps**:
* Explicit unit test coverage for the cache invalidation behavior (asserting a change in `FINGERPRINT_VERSION` changes the generated fingerprint) is implied by existing tests but not explicitly specified in the evaluation matrix.

## 6. Codebase Alignment
**Violations**:
* Failing to update `FINGERPRINT_VERSION` in `src/cache.rs` directly violates the codebase convention described on lines 22-24 of `src/cache.rs`: *"bump when the grouping prompt or schema changes ... so a cached plan from an older contract re-analyzes."*

**Alignment**:
* The rest of the spec matches codebase patterns flawlessly. The defensive parsing flow in `src/plan.rs:parse_defensive` and payload builders in `src/provider/ollama.rs` are respected.

## 7. Blind Spots
* **Cache Stale Plan Reuse**: If `FINGERPRINT_VERSION` is not bumped, old cached plans are reused. Those old plans may contain non-compliant or buggy structure if they were generated with models struggling with the unconstrained prompt.
* **Nested Aliasing**: If a model wraps the response in a wrapper key and uses the alias, like `{"result": {"commits": [...]}}`, the `recover_groups` function might not catch it if it only matches the top-level `commits` key. Normalizing the keys recursively or in DFS during group recovery is an extremely robust addition to defense-in-depth.

## 8. Verdict
**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback
1. **Critical (High Priority)**: Add a requirement to increment `FINGERPRINT_VERSION` from `2` to `3` in `src/cache.rs` as part of the sub-task implementing prompt changes. This ensures automatic invalidation of stale cache files.
2. **Implementation Detail (Medium Priority)**: Structure the helper inside `recover_groups` as a general-purpose `normalize_recovered_groups(mut groups: Value) -> Value` that walks the recovered array and maps `message` keys to `commit_message` keys only if `commit_message` is not already present. This ensures any recovered group array (even from bare arrays, wrappers, or DFS) is normalized uniformly.
3. **Robustness (Low Priority)**: In `recover_groups`, in addition to checking a top-level `commits` key, also check the standard wrapper keys (`"commit_plan"`, `"plan"`, `"result"`, etc.) for nested `commits` arrays (i.e. `inner.get("commits")`) to defend against wrapped near-miss responses.

All other parts of the specification are exceptional. Once these small additions are integrated, the implementation can proceed with high confidence.
