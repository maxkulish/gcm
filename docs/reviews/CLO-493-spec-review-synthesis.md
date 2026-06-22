# Spec Review Synthesis: CLO-493

**Synthesized**: 2026-06-22
**Pipeline**: lok spec-review

---

## Synthesis: CLO-493 Spec Review

Both external reviewers (Gemini, Ollama) succeeded with verdict **APPROVE_WITH_SUGGESTIONS**. Claude fallback was correctly skipped. The two reviews converge tightly on the core gaps and diverge mainly on additive suggestions.

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | JSON envelope schemas are not formally defined per status variant (`plan` / `noop` / `committed` / `error` / `fallback`). Implementation will drift without explicit field-level contracts. | Critical |
| 2 | **AC-6 fallback is ambiguous**: when grouping fails under `--yes --json` and a single commit *succeeds*, the envelope must represent both the fallback event and the commit outcome (hash/status). Schema must be standardized. | Critical |
| 3 | Spec does not mandate that **all** `GcmError` / `ProviderError` variants serialize to stdout as `status:"error"` under `--json`. Raw stderr strings would leak and break consumers. | High |
| 4 | **No Constraints section**, and no explicit stdout-purity rule: under `--json`, stdout must carry exactly one JSON object; all logs/warnings/traces (incl. `curated_index_warning`) go to stderr. | High |
| 5 | Log-level handling underspecified: precedence between new `GCM_LOG_LEVEL` and the legacy debug var, the default when unset, and the fact that the existing `debug_log!` macro has no level support. | Medium |
| 6 | Codebase uses custom `GcmError` (`src/error.rs`) + `ProviderError` (`src/provider/mod.rs`), **not** `anyhow`/`BackendErrorKind`. Spec must align to these (both confirm no current violation). `Plan` already derives `Serialize`. | Low (confirmation) |
| 7 | Evaluation table lacks a scenario for **error serialization under `--json`** (missing API key, unmerged conflicts, pre-commit hook / signing failure, fresh-plan validation failure). | Medium |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | Legacy debug env var name | `DEBUG_GCM=1` | `GCM_DEBUG` (read from `src/debug.rs`) | SKIPPED |
| 2 | Default log level when unset | `off` | Open question (`warn`/`error`/`off`?) | SKIPPED |
| 3 | Precedence rule | `GCM_LOG_LEVEL` overrides legacy var | Notes only that levels need a `debug.rs` refactor | SKIPPED |

> Resolution note: #1 is factual, not a judgment call. Ollama inspected `src/debug.rs`, so `GCM_DEBUG` is the likely-correct name — **verify in code before writing the spec**, since the precedence rule depends on naming the right variable.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Exit codes undefined** per JSON status (proposed AC-11: 0 for plan/committed/noop, non-zero for error). Critical for CI scripting. | Ollama | High |
| 2 | No **schema-version field** (`"v":1`); future changes will silently break consumers. | Ollama | High |
| 3 | `--dry-run` vs `--plan-only` JSON: are they identical (`status:"plan"`) or differentiated by a `mode` field? Recommends identical. | Ollama | Medium |
| 4 | **`output.rs` architecture undefined**: struct emitter vs `Output` trait (Human/Json) vs `OutputMode` enum into `execute()`. ST1 must pick one. | Ollama | Medium |
| 5 | **ST3 likely underestimated** (~6 execution paths to map: grouping / single / fallback / merge / dry-run / plan-only); 2h → 4h+. | Ollama | Medium |
| 6 | Concrete merged fallback schema proposed: `{status:"fallback", fallback:{reason}, commit:{status:"ok",hash,message}}`. | Gemini | Medium |
| 7 | `display_groups` (human output in `main.rs`) must be suppressed/redirected under `--json`. | Ollama | Low |
| 8 | Behavior unspecified for `--reset`, merge-commit state, and non-UTF8 paths under `--json`. | Ollama | Low |
| 9 | Intra-task dependencies not declared (ST3 depends on ST1+ST2; fallback work depends on CLO-492's `PlanError` Display being machine-readable). | Both (split) | Low |
| 10 | Optional `cached:true` field on cache-hit plans for CI reproducibility. | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (both reviewers agree; no NEEDS_REVISION, not unanimous APPROVE).

## Priority Actions

Ordered by severity; agreement items first.

1. **Define every JSON envelope schema explicitly** (Agreement #1) — document field layout for `plan`, `noop`, `committed`, `error`, `fallback`, including a `schema_version`/`v` field (Novel #2).
2. **Resolve AC-6 fallback schema** (Agreement #2) — adopt the merged structure (Novel #6) so a fallback that commits surfaces both `fallback.reason` and `commit.{hash,message}`.
3. **Mandate universal error serialization** (Agreement #3) — add to ST1/ST3 (or a new ST): map all `GcmError` + `ProviderError` variants to `status:"error"` with a defined `error.code` enum (`NonInteractive`, `NotARepo`, `Git`, `Provider`/`RateLimit`/`Auth`, `CommitFailed`, `Editor`, `EmptyMessage`).
4. **Add a Constraints section with stdout purity** (Agreement #4) — Must: only one JSON object on stdout; all logs/warnings (incl. `curated_index_warning`) to stderr; no third-party (`ureq`) stdout leakage. Must-not: async runtime; changing the `Plan` schema.
5. **Add AC-11 for exit codes** (Novel #1) — 0 for `plan`/`committed`/`noop`, non-zero for `error`; decide fallback's code.
6. **Specify logging semantics** (Agreement #5 / Disagreement #1-3) — verify the real env var name in `src/debug.rs` (`GCM_DEBUG` per Ollama), set precedence (`GCM_LOG_LEVEL` wins), define default level, and scope the `debug_log!` level refactor in ST4.
7. **Clarify `--dry-run` vs `--plan-only` output and `output.rs` architecture** (Novel #3, #4) — pick identical-output + one emitter design before ST1.
8. **Backfill evaluation scenarios** (Agreement #7) — error-serialization-under-json, fresh-plan validation failure, and re-estimate ST3 (Novel #5).
9. **Address remaining edge cases** (Novel #7-10) — `display_groups` suppression, `--reset`/merge/non-UTF8 behavior, task dependencies, optional `cached` flag.
