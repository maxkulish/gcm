# Spec Review: clo-517

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-29
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Clear, complete, and accurate.** The specification correctly identifies the root cause with verified evidence:

- **Root cause verified**: The hotfix document confirms the failure mode was reproduced and the fix was validated by testing with explicit prompt changes
- **Matches Linear description**: The problem statement accurately reflects the Linear task: "Multi-commit grouping fails for Ollama cloud passthrough models"
- **Self-contained**: Contains all necessary context including the observed malformed response shape (`{commits:[{message}]}`) vs expected shape (`{groups:[{commit_message}]}`)
- **Affected users clearly scoped**: "anyone using an Ollama cloud passthrough model (or any provider/model that ignores or weakly honors structured-output `format`/`response_format`)"
- **Line references accurate**: Verified that `GROUPING_SYSTEM_PROMPT` is at `src/provider/mod.rs:341`, `parse_defensive` at `src/plan.rs:80`, `recover_groups` at `src/plan.rs:189`, and `schema()` at `src/plan.rs:229`

**Minor gap**: The spec doesn't mention that Ollama cloud models are a relatively new feature (CLO-495), which would provide context for why this wasn't caught earlier. However, this doesn't affect the fix.

## 2. Acceptance Criteria Review

**Strong**:
- ✅ All criteria are specific and measurable
- ✅ Each criterion maps to a testable behavior
- ✅ References specific code locations (line numbers verified as accurate)
- ✅ Includes regression constraints ("all existing `plan.rs` parse tests still pass")
- ✅ Defense-in-depth approach captured (both prompt fix AND parser tolerance)
- ✅ Validation semantics preserved (recovered plans still pass through `validate`)

**Gaps**:
- **Missing criterion**: No explicit test that a response containing *both* `commits` and `groups` keys at top-level prefers `groups` (precedence is mentioned in edge cases but not in AC)
- **Missing criterion**: The doc-comment update for `GROUPING_SYSTEM_PROMPT` is mentioned in decomposition but not in acceptance criteria
- **Minor**: The "(Optional/docs)" criterion should probably be a "Prefer" constraint rather than optional AC

## 3. Constraints Check

**Aligned**:
- ✅ **Must: Keep `format` field unchanged** - verified in `ollama.rs:build_plan_payload` that this is a minimal, safe constraint
- ✅ **Must: Keep strict shape as primary contract** - `parse_defensive` tries direct `Plan` deserialize first, recovery is fallback
- ✅ **Must: Preserve validation semantics** - `validate` function enforces `MissingFirstMessage` on `groups[0]`, recovery shouldn't bypass this
- ✅ **Must-not: Change `schema()`** - verified that `plan::schema()` is used by OpenAI-compatible providers (Groq, OpenAI) and changing it would be unnecessary risk
- ✅ **Prefer: Implement per-group `message` → `commit_message` normalization in/near `recover_groups`** - aligns with existing recovery pattern (wrapper keys, DFS)

**Concerns**:
- **Potential conflict**: The spec says "Keep precedence: direct `Plan` parse first, then `groups`, then known wrappers / DFS, then the new `commits` alias" but the current `recover_groups` implementation wraps the result back through `from_value` - the normalization should happen *before* the re-wrap, not after. The decomposition correctly notes this but the constraint could be clearer about implementation location.
- **Implicit constraint not captured**: The existing `fenced_blocks` → `balanced_objects` → `whole content` candidate order in `candidates()` should not change - this is an implicit contract for recovery attempt order.

## 4. Decomposition Quality

**Well-scoped**:
- ✅ Sub-task 1 (Prompt change): Single file, clear scope, ~1-2 hours
- ✅ Sub-task 2 (Parser tolerance): Single file, focused change, ~1-2 hours  
- ✅ Sub-task 3 (Test + docs): Follows implementation, clear test patterns exist

**Issues**:
- **Missing sub-task**: The doc-comment update for `GROUPING_SYSTEM_PROMPT` (changing from "the structured-output schema enforces the shape" to reflect prompt-level specification) should be an explicit sub-task or clearly part of sub-task 1
- **Dependency clarification needed**: Sub-task 2 description says "Keep precedence: direct `Plan` parse first, then `groups`, then known wrappers / DFS, then the new `commits` alias" but this is already the natural order since `parse_defensive` tries direct deserialize first, then calls `recover_groups`. The `commits` alias should be added *inside* `recover_groups` after existing branches.

**Dependencies correctly identified**:
- 1 and 2 are independent ✓
- 3 depends on 2 ✓

## 5. Evaluation Coverage

**Covered**:
- ✅ Test #1: New unit test for `commits/message` shape
- ✅ Test #2: `message: null` on non-first groups
- ✅ Tests #3-4: Regression tests for existing recovery paths
- ✅ Test #5: Prompt shape + example presence (grep/code review)
- ✅ Test #6: Local-model payload unchanged
- ✅ Test #7: Full suite + lint
- ✅ Test #8: Manual real-model validation

**Edge cases covered**:
- ✅ Both `message` AND `commit_message` present (real key wins)
- ✅ `commits[0].message: null` still raises `MissingFirstMessage` via `validate`
- ✅ Response with both `commits` and `groups` (strict `groups` precedence preserved)
- ✅ Bare top-level array and existing wrapper-key shapes continue to recover

**Gaps**:
- **Missing test scenario**: A response with `commits` array containing a malformed group (e.g., missing `files` or `summary`) should still fail `Plan` deserialization after normalization - this validates that recovery doesn't bypass schema validation
- **Missing test scenario**: The prompt change should be tested to produce correct shape from a model that doesn't receive schema - this is covered by the manual real-model test but could have a unit test checking the prompt contains the example JSON

## 6. Codebase Alignment

**Violations**: None found. The spec correctly follows established patterns.

**Alignment**:
- ✅ **Recovery pattern**: The proposed `commits` alias handling follows the same pattern as existing wrapper-key handling in `recover_groups`
- ✅ **Validation separation**: The spec correctly identifies that `validate` is called *after* recovery, so validation semantics are preserved
- ✅ **Test pattern**: The existing `parse_defensive_*` tests provide clear patterns for the new tests
- ✅ **Error handling**: `PlanError::Parse` with descriptive message follows existing error taxonomy
- ✅ **Provider abstraction**: The fix is in shared `provider/mod.rs` prompt, not in Ollama-specific code - benefits all providers
- ✅ **Backend trait contract**: The `Provider` trait's `generate_plan` returns `Result<Plan, ProviderError>` after calling `parse_defensive` - this flow is preserved

**Verified constraints against source**:
- `build_plan_payload` in `ollama.rs:188` still sends `format: crate::plan::schema()` - confirmed line 188 in spec matches actual code structure
- `GROUPING_SYSTEM_PROMPT` doc-comment at line 337-338 says "structured-output schema enforces the shape" - this needs updating as noted in decomposition
- `recover_groups` at line 189-207 does NOT currently handle `commits` - confirmed

## 7. Blind Spots

**What the specification misses**:

1. **Logging/observability**: When recovery succeeds from a `commits`/`message` alias shape, there's no indication to the user that a non-canonical response was recovered. Should there be a debug log or trace? This would help diagnose future model behavior variations.

2. **Multiple near-miss shapes in the wild**: The spec addresses the observed `{commits:[{message}]}` shape, but other models might emit different near-miss shapes. The spec should clarify whether this is the *only* known alias shape or if others have been observed. The DFS fallback in `recover_groups` suggests the codebase anticipates nested variations.

3. **Performance impact**: Adding another recovery branch in `recover_groups` is minimal overhead, but the spec should confirm this is acceptable (it is, but not explicitly addressed).

4. **Documentation for model developers**: If Ollama cloud models consistently ignore `format`, should there be provider-specific documentation warning users? The "(Optional/docs)" criterion mentions a one-line note but doesn't specify where.

5. **Interaction with `schema()` version changes**: If `schema()` ever changes (new fields, different structure), both the prompt AND the recovery aliases would need updating. This coupling should be documented.

6. **Test coverage for the prompt itself**: The spec tests that the prompt *contains* the shape/example, but doesn't test that a model receiving only the prompt (no schema) would produce correct output. This is hard to test without a real model, but the manual test covers it.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is well-structured, accurately describes the problem and solution, correctly identifies the root cause with verified evidence, and follows established codebase patterns. The decomposition is appropriate and the test coverage is comprehensive.

The suggestions below are quality-of-life improvements that don't block implementation but would strengthen the specification.

## 9. Actionable Feedback

**Priority 1 (Should address before implementation)**:

1. **Add explicit acceptance criterion for precedence test**: Add a test that verifies `{"groups":[...], "commits":[...]}` prefers `groups` - currently only mentioned in edge cases section.

2. **Add explicit sub-task for doc-comment update**: Move the doc-comment change from decomposition to an explicit sub-task or AC. The current doc-comment "the structured-output schema enforces the shape, so the prompt carries only the grouping rules" will become inaccurate after the fix.

**Priority 2 (Nice to have)**:

3. **Add debug-level logging for recovery**: When `recover_groups` normalizes `commits`/`message`, consider a `log::debug!` or `tracing::debug!` indicating the alias was used. This aids future debugging.

4. **Add test for malformed normalized response**: Add a test that `{"commits":[{"message":"x"}]}` (missing required `files` and `summary`) still fails deserialization after normalization - validates recovery doesn't bypass validation.

5. **Clarify implementation location in constraint**: The "Prefer" constraint about implementing normalization in/near `recover_groups` should clarify that normalization happens *inside* `recover_groups` before the `from_value` call, not as a separate pass.

**Priority 3 (Documentation)**:

6. **Specify where the cloud-model note goes**: The "(Optional/docs)" criterion should specify the location - likely in `src/provider/ollama.rs` module doc-comment and/or a note in the GROUPING_SYSTEM_PROMPT doc-comment explaining why the prompt restates the schema.

7. **Document the schema/prompt coupling**: Add a note in the code or a comment that `GROUPING_SYSTEM_PROMPT` and `schema()` describe the same shape and must be kept in sync.
