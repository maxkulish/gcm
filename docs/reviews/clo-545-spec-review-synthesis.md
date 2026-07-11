# Spec Review Synthesis: clo-545

**Synthesized**: 2026-07-10
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | `keep_chat_model_excludes_non_text_for_openai_groq` is missing from both the decomposition (ST1) and the evaluation table, yet it asserts on legacy strings (`gpt-5.4-mini`, `gpt-4o`) that will fail AC4. Must be retargeted and listed. | High |
| 2 | No explicit AC/sub-task covers updating all unit-test assertions that carry legacy model strings; test-refactor scope in ST1/ST2 is understated. Gemini: test retargeting; Ollama: proposes AC8. | High |
| 3 | Migration/transition friction for existing users is undocumented. Upgrading resolves default to `gpt-5.6-luna`, which is not in a stale `gcm.toml` whitelist → validation error; recovery requires `gcm provider` / `--reconfigure`. Needs a release note. | High |
| 4 | Config fixtures / test constants carry many legacy model references (Ollama estimates ~15+ in `config.rs`); blast radius of the string sweep is underestimated in the spec. | Medium |
| 5 | Verdict: both reviewers reach **APPROVE_WITH_SUGGESTIONS**; spec is fundamentally sound and may proceed. | Info |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | How to handle temperature-rejecting models after removing the o-series reasoning path | Fully **delete** `apply_model_params` / `apply_model_params_resolve` and inline `temperature` directly into each payload builder — maximize simplification | Keep a small guard: introduce `is_temperature_rejecting_model()` rather than a revived o-series branch — preserve future-proofing | SKIPPED (external reviewer succeeded) |

No direct contradictions elsewhere; the two reviews are largely complementary. The only genuine tension is the escalate-path design above (delete-and-inline vs. retain a guard helper).

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | Eval table Row 1 command `cargo test default_model` matches no test; actual test is `provider_defaults_and_tokens`. | Gemini | Medium |
| 2 | Fallback test only asserts the list *contains* the default; it does not verify the list is *exactly* `["gpt-5.6-luna", "gpt-5.6-terra"]`. | Gemini | Medium |
| 3 | Spec **assumes** `gpt-5.6-luna` supports strict `json_schema`; should be verified, not assumed. | Ollama | Medium |
| 4 | Cache-key stability: existing `openai:gpt-5.4-mini` cached plans won't match post-migration (expected, but document it). | Ollama | Low |
| 5 | AC7 live smoke test lacks measurable pass/fail criteria (suggested: "JSON parses without fallback to single-commit"). | Ollama | Low |
| 6 | No documented rollback plan if live testing reveals blocking issues. | Ollama | Low |
| 7 | README needs a migration note for o-series behavior change (`--model=o3-mini` scripts break). | Ollama | Low |
| 8 | Future: diff-budget optimization for `gpt-5.6-luna`'s larger context window. | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Both external reviewers approved with suggestions; no reviewer returned NEEDS_REVISION. Claude fallback was correctly skipped (external reviewers succeeded). The spec is implementable as-is; the items below tighten test coverage and migration handling before/during implementation.

## Priority Actions

**P1 — Address before implementation (agreement items first)**
1. Add `keep_chat_model_excludes_non_text_for_openai_groq` to ST1 and the evaluation table; retarget its legacy-string assertions so AC4 passes. *(Agreement #1)*
2. Add an acceptance criterion (Ollama's AC8) requiring every unit-test assertion with legacy model strings to be updated; expand the sweep scope to explicitly include `src/config.rs` fixtures. *(Agreement #2, #4)*
3. Document the existing-user whitelist transition: on upgrade, a stale `gcm.toml` whitelist without `gpt-5.6-luna` triggers a validation error; add a release/README note pointing to `gcm provider` / `--reconfigure`. *(Agreement #3)*
4. Decide the escalate-path design: full inline-and-delete of `apply_model_params` (Gemini) vs. a `is_temperature_rejecting_model()` guard (Ollama). *(Disagreement #1)*

**P2 — Fix during implementation**
5. Correct eval Row 1 command to `cargo test provider_defaults_and_tokens`. *(Novel #1)*
6. Add a strict-equality fallback assertion (`assert_eq!(static_fallback_models(ProviderId::Openai), vec!["gpt-5.6-luna","gpt-5.6-terra"])`). *(Novel #2)*
7. Verify (don't assume) `gpt-5.6-luna` strict `json_schema` support; document cache invalidation for legacy `openai:gpt-5.4-mini` plans. *(Novel #3, #4)*
8. Add a README migration note for the o-series behavior change. *(Novel #7)*

**P3 — Nice to have**
9. Give AC7 measurable pass criteria and note a rollback plan for live-test blockers. *(Novel #5, #6)*
10. Consider diff-budget tuning for the larger context window. *(Novel #8)*
