# CLO-801: Return group assignments by index so the grouping plan stops echoing every changed path

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-801
**Spec**: docs/specs/2026-11-21-clo-801-group-index.md (approved blob 654c1c45)
**Started**: 2026-11-21
**Last Updated**: 2026-11-21

---

## Current Status: Implemented, live checks open

**Overall Progress**: sub-tasks 1-4 done; sub-task 5 (live re-measure) is owner-run
**Branch**: feat/clo-801-group-index
**Commit**: 38d0911

---

## Completed Sub-tasks

- [x] 1. Inventory index - `src/diff.rs`: each `InventoryEntry` carries `i`, its position in `changed`
- [x] 2. Reply type and resolve - `src/plan.rs`: `PlanReply`/`ReplyGroup`/`FileRef` (integer index or string path), `resolve`, `PlanError::UnknownIndex`, integer schemas
- [x] 3. Prompt and providers - `src/provider/*.rs`: prompt and header ask for `i` values; `generate_plan` returns `PlanReply`; Anthropic shortcuts parse `PlanReply`
- [x] 4. Build plan, budget, cache, integration tests - `resolve_reply` in `src/main.rs` with the debug ref-count line; index-echo reserve in `src/budget.rs`; `FINGERPRINT_VERSION` 4; `tests/grouping_budget.rs` 515-file grouped Anthropic case, 2,000-file refusal, debug-line case, fallback window 35,000
- [ ] 5. Live re-measure (AC10 Vertex, AC11 Anthropic, Ollama run) - owner-run

---

## Technical Decisions

- **Resolve outside the provider**: `build_plan` maps indices against the real `changed` slice, never against the possibly redacted prompt text; `resolve_reply` is the unit-testable seam.
- **Index echo estimate**: `estimate_tokens` of `0..n` joined by `,`; 515 files are about 975 tokens, 2,000 files about 4,450 (over the 4,096 cap).

---

## Important Findings

- The inventory order is git status order (sorted by path), not file creation order. The 515-file integration test therefore checks each resolved path against the inventory entry the request carried.
- Row 24a verified: with `GCM_CONTEXT_WINDOW_TOKENS=35000` the grouping prompt still cannot fit and only the fallback message request is sent.

---

## Next Steps

- Codex + Gemini validation gate, then PR
- Owner runs AC10, AC11 and the Ollama run. If an Ollama reply fails to parse before `build_plan` receives it, record the failure with counts unavailable, not zero counts.
