# Spec Review: clo-545

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-07-10
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Clear and well-defined.** The specification accurately identifies the stale OpenAI model catalog and correctly links to the relevant code locations. The distinction between `gpt-5.6-luna` (new default) and `gpt-5.6-terra` (fallback) is clear.

## 2. Acceptance Criteria Review

**Strong**: AC1-AC6 are specific and testable.

**Gaps**: Missing explicit AC for test assertion updates and config fixture updates. AC7 (live smoke) lacks measurable pass/fail criteria.

## 3. Constraints Check

**Aligned with codebase patterns**. Must/Must-not/Prefer/Escalate structure is well-used.

**Concerns**: Escalate path could specify implementation approach more clearly (e.g., suggest `is_temperature_rejecting_model()` check vs revived o-series branch).

## 4. Decomposition Quality

**Well-scoped sub-tasks** that are largely independent and file-disjoint.

**Issues**: ST1 is slightly larger than claimed (~4 test locations). ST2 scope understated for test refactoring. Missing sub-task for `keep_chat_model` integration verification. Dependency order could be clarified (recommend ST1 → ST2 → ST3).

## 5. Evaluation Coverage

**Covered**: Tests 1-5 have clear expected results. Test 6 appropriately scoped as owner-run.

**Gaps**: Missing test for `keep_chat_model` with GPT-5.6 models. Missing config fixture round-trip verification. Test 6 lacks explicit pass criteria (suggest: "JSON parses without fallback to single-commit").

## 6. Codebase Alignment

**Correct**: Line number references verified. Follows `Provider` trait contract. Aligns with existing error handling and `strip_think()` mitigation.

**Assumption to call out**: Spec assumes `gpt-5.6-luna` supports strict json_schema; OpenAI docs confirm this, but should be explicitly verified, not assumed.

## 7. Blind Spots

- Cache key stability: existing `openai:gpt-5.4-mini` cached plans won't match post-migration (expected, but worth documenting)
- `gcm provider` wizard model list still works with new fallback
- No rollback plan documented if live testing reveals blocking issues
- Breaking change for o-series users with `--model=o3-mini` in scripts should have migration note in README
- `config.rs` test fixtures contain ~15+ model string references; blast radius underestimated

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS** — specification is fundamentally sound and can proceed. Minor gaps would improve implementation clarity and reduce missed changes.

## 9. Actionable Feedback

**Priority 1 (Address before implementation)**:
1. Add AC8: All unit test assertions with legacy model strings updated
2. Expand AC4 to explicitly include `src/config.rs` test fixtures (~15+ occurrences)
3. Clarify Escalate path: recommend `is_temperature_rejecting_model()` function vs revived branch

**Priority 2 (During implementation)**:
4. Document cache invalidation behavior
5. Add README migration note for o-series behavior change
6. Add `keep_chat_model` test verification for GPT-5.6 models

**Priority 3 (Future)**:
7. Consider diff budget optimization for `gpt-5.6-luna`'s larger context window
8. Add measurable pass criteria for live smoke test
