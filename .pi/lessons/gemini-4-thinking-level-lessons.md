# Lessons: Gemini 4 thinking level

Source: branch `fix/gemini-4-thinking-level` (commit cfa6653)
Date: 2026-11-17

---

## L1 - The lowest Gemini thinkingLevel is model-specific, so check it for every new model family

**Source incident**: `gcm` with Vertex `gemini-4-hydrogen` failed with HTTP 400 "Thinking level is unsupported: THINKING_LEVEL_MINIMAL" on both the grouping request and the single-commit fallback. `src/provider/gemini.rs` sent `MINIMAL` to every Gemini model. Gemini 4, `gemini-3.8-flash`, `gemini-3.1-pro` and `gemini-3-pro` accept `LOW` as their lowest level, and Gemini 4 requires the field, so omitting it is not a fallback.

**Rule**: When a new Gemini model family ships, check the per-model level table in the AI Studio thinking guide (aistudio.google.com/docs/thinking). If the family rejects `MINIMAL`, add its prefix to `NO_MINIMAL_PREFIXES`. Add a `WINDOWS` entry in `src/budget.rs` for the same family in the same change.

**How to apply**: `thinking_level(model)` in `src/provider/gemini.rs` picks the level by prefix, the same way `src/budget.rs` maps models to windows. Do not replace it with a retry on 400: a retry doubles latency and hides the next rejection. Extend `thinking_level_is_low_where_minimal_is_rejected` with the new prefix.
