# Spec Review Synthesis: clo-564

**Synthesized**: 2026-07-22
**Pipeline**: lok spec-review

---

## Review Synthesis: CLO-564 Vertex Live Discovery Spec

**Source availability**: Gemini succeeded; Ollama/Codex (glm-5:cloud) failed with empty output; Claude fallback skipped. Synthesis below is from the single valid source (Gemini) - cross-reviewer agreement/disagreement analysis is not applicable this round.

## Findings (Single Source: Gemini)

| # | Finding | Severity |
|---|---------|----------|
| 1 | `resolved_base_url_with` in `src/provider/models.rs` groups `ProviderId::Vertex` under `ProviderId::Google`, resolving to the Gemini API host and Gemini env vars. Left unchanged, Vertex discovery hits the wrong domain entirely. Spec must add a task to split Vertex into its own branch: default `https://aiplatform.googleapis.com`, env override `GCM_VERTEX_BASE_URL`. | Critical |
| 2 | Adding the `project` parameter to `fetch_supported_models` / `fetch_supported_models_with` breaks every existing mock/transport test call site (~10-15 across OpenAI, Anthropic, Google, Ollama suites). Decomposition Task 4 needs an explicit subtask to update them to pass `None`. | High |
| 3 | AC5 doesn't explicitly require parsing the `"publisherModels"` response key (vs `"models"`) and de-prefixing model names, even though the decomposition mentions it. Criteria and decomposition should match. | Medium |
| 4 | No test scenario for a missing/empty/malformed `"publisherModels"` key - parser should degrade to an empty vector (triggering static fallback) rather than erroring. | Medium |

## What the Reviewer Confirmed as Sound

- Problem statement accurate and aligned with CLO-564, including the verified endpoint and auth requirements
- AC1/AC2 separation of concerns (no subprocess in `models.rs`, token resolution in `vertex.rs`)
- AC3/AC4 catalog refresh preserving the fallback-contains-default invariant
- AC6 whitelist backwards compatibility without schema bump
- Decomposition granularity and dependency ordering (`1 -> 2 -> {3, 4}`)
- Hermetic test strategy via `GCM_VERTEX_BASE_URL` seam; clean reuse of the CLO-547 transport seam and existing `resolve_access_token` / `probe_adc` stubs
- No structural violations of the provider abstraction

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

(Sole valid reviewer verdict: APPROVE_WITH_SUGGESTIONS. Note: single-source verdict carries less confidence than a cross-referenced one - findings #1 and #2 are code-level claims worth verifying against `src/provider/models.rs` before revising the spec.)

## Priority Actions

1. **(Critical)** Add a spec task to refactor `resolved_base_url_with`: separate `ProviderId::Vertex` from `ProviderId::Google` with `&["GCM_VERTEX_BASE_URL"]` overrides and `https://aiplatform.googleapis.com` default.
2. **(High)** Extend Decomposition Task 4 with a subtask updating all existing `fetch_supported_models_with` test call sites to pass `None` for the new `project` parameter.
3. **(Medium)** Amend AC5 to explicitly require parsing the `"publisherModels"` key and de-prefixing returned model IDs.
4. **(Medium)** Add a parser unit test asserting graceful handling of missing/empty/malformed `"publisherModels"` (empty vec -> static fallback, no panic).
