# Spec Review Synthesis: clo-517

**Synthesized**: 2026-06-29
**Pipeline**: lok spec-review

---

Verified: `FINGERPRINT_VERSION = 2` (line 26) with the doc-comment (lines 22-24) explicitly stating to bump it when the grouping prompt changes. Since the spec modifies `GROUPING_SYSTEM_PROMPT`, Gemini's critical finding is **confirmed against source**. Synthesis below.

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | Problem statement is clear, complete, accurate; root cause (`{commits:[{message}]}` vs `{groups:[{commit_message}]}`) verified with evidence; line references (`mod.rs:341`, `plan.rs:80/189/229`) correct | Strength |
| 2 | Decomposition is well-scoped: Sub-task 1 (prompt) and Sub-task 2 (parser) are independent/parallel <2h; Sub-task 3 (tests+docs) correctly sequential | Strength |
| 3 | Constraints aligned: keep `format` unchanged for local GGUF, strict `Plan` parse as primary path, don't touch `schema()`, preserve `validate` semantics | Strength |
| 4 | Doc-comment for `GROUPING_SYSTEM_PROMPT` ("the structured-output schema enforces the shape") becomes inaccurate after the fix and must be updated; it is in decomposition but missing as an explicit sub-task/AC | Medium |
| 5 | Missing explicit AC/test that `{groups, commits}` both present prefers `groups` (only in edge-cases, not AC) | Medium |
| 6 | Normalization (`message`→`commit_message`) must happen *inside* `recover_groups` before the `from_value` re-wrap, only when `commit_message` absent; both reviewers want this location pinned down | Medium |
| 7 | Missing test: `{"commits":[{"message":"x"}]}` lacking required `files`/`summary` still fails deserialization after normalization (proves recovery doesn't bypass schema) | Low-Med |
| 8 | Both verdicts: **APPROVE_WITH_SUGGESTIONS** | — |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | `FINGERPRINT_VERSION` bump | **Critical** — spec omits required bump 2→3; violates `cache.rs:22-24` convention | Not raised | SKIPPED — but verified against source: Gemini is correct |

No substantive contradictions; the only divergence is coverage (Gemini caught the cache issue, Ollama did not).

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **`FINGERPRINT_VERSION` must bump 2→3** — confirmed against `src/cache.rs:26` + doc-comment. Without it, stale plans generated under the old prompt contract are reused | Gemini | **High** |
| 2 | Make helper a general `normalize_recovered_groups(Value) -> Value` so all recovery paths (bare array, wrappers, DFS) normalize uniformly | Gemini | Medium |
| 3 | Defend against nested/wrapped near-miss (`{"result":{"commits":[...]}}`) — check wrapper keys for inner `commits`, not just top-level | Gemini | Low |
| 4 | Add `tracing::debug!` when `recover_groups` uses the `commits`/`message` alias (observability for future model drift) | Ollama | Low |
| 5 | Document the `GROUPING_SYSTEM_PROMPT` ↔ `schema()` coupling (must stay in sync) | Ollama | Low |
| 6 | Spec should state whether `{commits:[{message}]}` is the *only* observed near-miss or others exist | Ollama | Low |
| 7 | Specify location for the "(Optional/docs)" cloud-model note (`ollama.rs` module doc / prompt doc-comment) | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Both reviewers approve. No NEEDS_REVISION. The one High-severity item (`FINGERPRINT_VERSION`) is a real codebase-convention violation but is a one-line addition to the spec, not a structural flaw — it strengthens rather than blocks.

## Priority Actions

1. **[High — confirmed vs source]** Add a requirement to bump `FINGERPRINT_VERSION` from `2` to `3` in `src/cache.rs` as part of Sub-task 1. The doc-comment at `cache.rs:22-24` mandates this when the grouping prompt changes; omitting it reuses stale plans built under the old prompt contract. Add a unit test asserting the fingerprint changes.
2. **[Medium — agreed]** Promote the `GROUPING_SYSTEM_PROMPT` doc-comment update to an explicit sub-task/AC (current wording about schema-enforced shape becomes false).
3. **[Medium — agreed]** Add explicit AC + test: `{groups, commits}` both present → `groups` wins (precedence preserved).
4. **[Medium — agreed]** Pin the implementation location: normalize `message`→`commit_message` *inside* `recover_groups` before `from_value`, only when `commit_message` is absent. Prefer Gemini's general-purpose `normalize_recovered_groups(Value) -> Value` so all recovery paths normalize uniformly.
5. **[Low-Med — agreed]** Add test: normalized `commits` missing required `files`/`summary` still fails deserialization (recovery doesn't bypass schema).
6. **[Low — Gemini]** Defend against wrapped near-miss (`{"result":{"commits":[...]}}`) by checking wrapper keys for inner `commits`.
7. **[Low — Ollama]** Add `tracing::debug!` on alias recovery; document `GROUPING_SYSTEM_PROMPT`↔`schema()` coupling; specify location of the cloud-model docs note.
