# Design Review Synthesis: CLO-799 enabled-model-pinning

**Synthesized**: 2026-09-16

**Pipeline**: claude synthesis over the single valid reviewer (Gemini); lok design-review synthesis step failed on an undefined `steps.claude_fallback.*` variable when the fallback step was skipped



---



## Reviewer Status

| Reviewer | Status | Detail |
|---|---|---|
| Gemini 3.5 Flash | ✅ Valid | `APPROVE_WITH_SUGGESTIONS`, 3 prioritized items; delivered via direct opencode after the lok synthesis step failed on a template bug |
| Ollama | ⚠️ Failed | Empty output, no findings recovered |
| Claude fallback | ➖ Not run | Not needed - Gemini produced valid output |

## Source

Single review. Findings below are Gemini's, each cross-checked against `docs/designs/clo-799-enabled-model-pinning.md` and the code it names (`src/config/mod.rs`, `src/provider/models.rs`, `src/provider/identity.rs`). No second reviewer was available to corroborate, so the grounding check carries the weight.

## Key Findings

| # | Finding | Severity |
|---|---|---|
| 1 | **Empty-selection panic in `initial_default_model`** - not reproducible. The function already returns `Option<String>` and ends in `selected.first().cloned()`, which yields `None` on an empty slice (`src/config/mod.rs:703-716`). Design section C only reorders the preference chain and keeps the same return type. No panic path exists. | Invalid |
| 2 | **Unbounded known-model list in the rejection message** - real but bounded to near-zero. The largest static catalog is Google/Vertex at 6 short ids; OpenAI is 2 (`OPENAI_SUPPORTED_MODELS`, `src/provider/identity.rs:150`), Anthropic 3, Groq 3, Ollama empty. Worst case is roughly one wrapped line, not a wall of text. Worth a note only if the catalogs grow. | Low |
| 3 | **Canonical normalization consistency** - already the design's stated rule, and the mechanism exists. `canonicalize_model` (`src/config/mod.rs:518-526`) strips Google's `models/` prefix and appends Ollama's `:latest`; `model_is_enabled` and `wizard_model_list` both compare through it. The design says "canonical comparison, same rule as membership" for the new known-but-not-enabled diff. The finding is a correct constraint restated, not a gap - it becomes a review point at implementation, not a design change. | Low (already specified) |

Nothing in the review contradicts the design's core decisions: keeping the CLO-516 D4/AC-6 whitelist gate, staying offline on the error path, and holding `static_fallback_models` at `pub(crate)`.

## Verdict

**APPROVE_WITH_SUGGESTIONS**

No reviewer returned `NEEDS_REVISION`. Gemini approved the architecture explicitly; the three actionable items are one factual miss and two items already covered or negligible in scope. The design is implementable as written.

## Priority Actions

1. **Enforce canonical comparison in the new catalog diff** (item 3) - when computing known-but-not-enabled entries in `model_is_enabled`, route both sides through `canonicalize_model(id, …)` exactly as the membership check does. This is the one item that can silently produce a wrong message (a `models/gemini-3.5-flash` enabled entry listed again as "known"). Implementation-time check, no design change.
2. **Keep the known-model clause on one visual line** (item 2) - with ≤6 short ids the current phrasing is fine; if a catalog later exceeds ~8 entries, truncate and defer to `gcm provider`. Track as a note, not a blocker.
3. **No action on item 1** - the empty-selection concern is already handled by the existing `Option<String>` signature. Worth one sentence in the design's section C making the `None`-on-empty behavior explicit, so a future reader does not re-raise it.

---

━━━ CLO-799 · DESIGN REVIEW - approved with suggestions ━━━━━━━━━━━━

✅ DONE
   Synthesis complete: 1 of 2 reviewers valid (Gemini), verdict APPROVE_WITH_SUGGESTIONS
   All 3 Gemini findings cross-checked against the design doc and source; 1 invalid, 2 low

⚠️ NOT DONE
   Ollama reviewer returned empty output - no second opinion on this design
   Only the single-reviewer path validated the design's core trade-offs

👉 YOUR MOVE
1. [proceed]   Accept the verdict and move CLO-799 to the plan phase, carrying the canonical-comparison note into implementation   ← recommended
2. [rerun]     Re-run the design review with the Ollama reviewer fixed, for a corroborating second opinion
