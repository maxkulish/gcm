# Spec Review Synthesis: clo-515

**Synthesized**: 2026-06-26
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Early dispatch is mandatory.** `Commands::Status` must be intercepted in `main.rs:run()` before `ensure_configured()` / onboarding / `apply_to_env`, mirroring `Commands::Config`. Otherwise status blocks on onboarding or corrupts env/config attribution. | High |
| 2 | **`apply_to_env` must never be called** in the status path — it writes inline keys to the environment and destroys source attribution. | High |
| 3 | **AC-7 Ollama activation is ambiguous** and needs a concrete definition (no key to check, so "always reachable" is meaningless for activation state). | High |
| 4 | **Add a test for invalid `GCM_PROVIDER`** (e.g. `=bogus`) — status must report the error gracefully, not crash or swallow it. | Medium |
| 5 | **Add a test for malformed/unreadable config** (`load()` returns `None`) — should fall back to env-derived state and report config unusable. | Medium |
| 6 | **Reuse existing pure helpers** (`key_env_var`, `env_plan`, `config_path_from`); attribution logic should be pure and unit-testable. Spec already does this well. | Low (confirmation) |
| 7 | **JSON/stdout segregation is correct** — `v: 1` schema contract, jq round-trip test, separate `StatusReport` struct rather than overloading `Envelope`. | Low (confirmation) |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | **Masked key suffix (`…<last4>`)** | **Remove entirely** — high-entropy suffix leaks key-space and trips security scanners; show only `set`/`not set` + source | Silent (accepts AC-4 masking as a strength) | Skipped |
| 2 | **ST2 sizing** | Well-scoped, no issues | **Undersized** — exposing `default_model()`/`model_env_vars()` + refactor may be 3-4h; split into ST2a/ST2b | Skipped |

Recommendation on #1: side with Gemini — drop the `…<last4>` option. The security downside outweighs the marginal debugging value, and "set (env NAME)" already gives provenance.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Ollama `:cloud` egress.** Models ending `:cloud` proxy off-machine and are NOT zero-egress; detect and flag (e.g. `zero_egress` bool) in both outputs | Gemini | Medium |
| 2 | **`config_path()` returns `None`** (OS config dir unresolvable) must be handled, not panic | Gemini | Medium |
| 3 | **Gemini dual-env precedence** (`GCM_GEMINI_MODEL` > `GCM_GOOGLE_MODEL`) needs an explicit test | Gemini | Medium |
| 4 | **Missing exit-code AC** — propose AC-9: exit 0 on success, exit 1 only on provider-selection failure / unrecoverable config | Ollama | Medium |
| 5 | **Ollama endpoint precedence chain** (`GCM_OLLAMA_BASE_URL` → `OLLAMA_HOST` → config `endpoint` → `localhost:11434`) is more complex than cloud key attribution; ST3 glosses it — give it a dedicated `ollama_endpoint_source()` fn | Ollama | Medium |
| 6 | **Provider output order unspecified** — pin to canonical `cloud_then_ollama()` order in AC-3 | Ollama | Low |
| 7 | **Forward-compat note** — document that JSON consumers ignore unknown fields as schema evolves | Ollama | Low |
| 8 | **Insecure config permissions (0644)** — decide whether status surfaces this, matching existing `load()` warning | Ollama | Low |
| 9 | **Performance AC** — < 100ms since no network/diff | Ollama | Low |
| 10 | **`gcm version` source** — confirm `cli::VERSION` and give it an explicit task entry | Both (minor) | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (Gemini: APPROVE_WITH_SUGGESTIONS, Ollama: APPROVE_WITH_SUGGESTIONS — no NEEDS_REVISION). The spec is fundamentally sound and well-aligned with the codebase. Address the items below before implementation. Claude fallback was correctly skipped (both external reviewers succeeded).

## Priority Actions

**P1 — Block implementation (agreement items first):**
1. Document early dispatch of `Commands::Status` in `main.rs:run()` before `ensure_configured()`/onboarding/`apply_to_env` (Agreement #1, #2).
2. Define AC-7 Ollama activation concretely: activated iff listed in config `providers` OR `OLLAMA_HOST`/`GCM_OLLAMA_BASE_URL` set; otherwise report endpoint status, not "active by default" (Agreement #3 + Gemini action 1).
3. Add AC-9 for exit codes: 0 on success, 1 only on provider-selection failure / unrecoverable config (Novel #4).
4. Add test cases: invalid `GCM_PROVIDER`, malformed config (Agreement #4, #5).

**P2 — Address during implementation:**
5. Remove the `…<last4>` masked-suffix option from AC-4 (Disagreement #1 → side with Gemini).
6. Add dedicated `ollama_endpoint_source()` with the full precedence chain to ST3 (Novel #5).
7. Handle `config_path()` → `None` without panicking (Novel #2).
8. Pin provider output order to `cloud_then_ollama()` in AC-3 (Novel #6).
9. Add Gemini dual-env precedence test (`GCM_GEMINI_MODEL` > `GCM_GOOGLE_MODEL`) (Novel #3).
10. Consider splitting ST2 into ST2a (trivial accessor exposure) / ST2b (`resolve_model_with_source`) for time accuracy (Disagreement #2).

**P3 — Consider for revision:**
11. Ollama `:cloud` non-zero-egress detection + `zero_egress` field (Novel #1).
12. Forward-compat note on unknown JSON fields; insecure-permissions reporting; performance AC; explicit `cli::VERSION` task entry (Novel #7, #8, #9, #10).
