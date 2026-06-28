# CLO-516 Implementation Plan: Interactive `gcm provider` wizard (cliclack)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-516
**Design Document**: docs/designs/clo-516-interactive-provider-wizard.md (Finalized)
**Architecture Reference**: docs/adrs/001-foundational-architecture-decisions.md (Decisions 2, 4, 11)
**Created**: 2026-06-28
**Overall Progress**: 42% (11/26 top-level tasks completed) — Phases 1-2 done (24603cc, e5c85a5)

---

## Architecture Context

Adds a new `gcm provider` subcommand: a cliclack wizard that picks a provider, fetches its
live model list (static fallback), multiselect-filters which models to enable, and chooses one
default. Introduces a per-provider `models: Vec<String>` enabled-set whitelist (config `version`
1 -> 2 migration) and enforces it at runtime in `main.rs::ensure_configured()` while the loaded
`Config` is still in scope. The synchronous `Provider` trait (ADR-001 Decision 2) is untouched;
model-fetching is a free function dispatched by `ProviderId`. Build the testable logic as pure
functions; keep cliclack confined to a thin IO shell (it reads `/dev/tty`, so the TUI itself is
manual-verify only). Implement strictly Phase 1 -> 4 so each layer is green before the next.

---

## Tasks

### Phase 1: Config schema + enforcement (pure; no TUI, no network) — D3, D4, D8

- [x] Task 1: Add the enabled-models field + version bump (`src/config.rs`)
  - [x] Subtask 1.1: Add `pub models: Vec<String>` to `ProviderConfig` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  - [x] Subtask 1.2: Bump `CONFIG_FORMAT_VERSION` 1 -> 2
  - [x] Subtask 1.3: Update the test `pc`/`pcm` helpers + any literal `ProviderConfig {…}` constructions to include `models: vec![]`
- [x] Task 2: Migration in `parse_config` (`src/config.rs`)
  - [x] Subtask 2.1: Accept `version == 1` (stamp `cfg.version = CONFIG_FORMAT_VERSION`, leave `models` empty) and `version == 2` natively; reject 0 / >2 as `WrongVersion`
  - [x] Subtask 2.2: Force `render_config` to serialize `version = CONFIG_FORMAT_VERSION` regardless of the in-memory value (closes the version-write trap, pt 9)
  - [x] Subtask 2.3: Bump literal `version = 1` -> `2` in `sample_toml_template()` and `non_tty_instructions()`; add a `models = [...]` example to `commented_reference()`
- [x] Task 3: Pure enforcement helper + canonicalization (`src/config.rs`)
  - [x] Subtask 3.1: `model_is_enabled(cfg, id, model) -> Result<(), String>` (empty `models` = Ok; non-empty = membership; error names the model + lists the set + `gcm provider`)
  - [x] Subtask 3.2: `canonicalize_model(id, model)` (Gemini strip `models/`, Ollama tagless -> `:latest`, trim; no case-fold); apply on both sides of the membership check (pt 17)
- [x] Task 4: Provider-config merge + cross-wizard preservation (`src/config.rs`) — D8
  - [x] Subtask 4.1: `merge_provider_config(existing: Option<&Config>, updated: ProviderConfig, make_default: bool) -> Config` (replace one by `id`, append if absent, preserve the rest, set default) (pt 1)
  - [x] Subtask 4.2: Make `run_wizard`/`build_config` load the existing config and carry forward each re-enabled provider's existing `models` (and inline default `model`) so `gcm config`/`--reconfigure` never erase a whitelist (pt 2)
- [x] Task 5: Wire enforcement into the runtime (`src/main.rs`)
  - [x] Subtask 5.1: In `ensure_configured()`, after `apply_to_env(&cfg)` (cfg still in scope), compute `id = pick_provider_id(...)` + `(model,_) = resolve_model_with_source(...)`, call `model_is_enabled(&cfg, id, &model)` mapped to `GcmError::Config`
  - [x] Subtask 5.2: Confirm the `pub`/`pub(crate)` visibility of `pick_provider_id` / `resolve_model_with_source` (widen if needed)
- [x] Task 6: Phase 1 unit tests (`src/config.rs` `#[cfg(test)]`)
  - [x] Subtask 6.1: migration (v1 loads + stamps v2; v2 round-trips with `models`; v0/v3 rejected); `render_config` writes `version = 2`
  - [x] Subtask 6.2: enforcement matrix (empty = allow-any; non-empty rejects out-of-set; canonicalized `llama3` matches `llama3:latest`; Gemini `models/x` vs `x`)
  - [x] Subtask 6.3: `merge_provider_config` preserves other providers + sets default; `run_wizard`/`build_config` preserve existing `models`

### Phase 2: Model fetching + hygiene (sync, fallback, injectable) — D2, D6, D7

- [x] Task 7: GET transport (`src/provider/http.rs`)
  - [x] Subtask 7.1: `get_json` (small GET request struct: endpoint + `auth` + `extra_headers`, no payload) reusing classify/timeout machinery
  - [x] Subtask 7.2: Short-timeout / low-retry path for wizard fetches (don't stall the spinner); large first page where paginated (`pageSize=1000`/`limit=1000`)
- [x] Task 8: `ModelFetchOutcome` + dispatcher (`src/provider/models.rs` NEW) — D7, pt 10
  - [x] Subtask 8.1: `pub struct ModelFetchOutcome { models, source, warning }` + `enum ModelSource { Live, Fallback }`
  - [x] Subtask 8.2: `fetch_supported_models(id, key, endpoint) -> ModelFetchOutcome` orchestrating: no-key short-circuit (pt 3) -> GET -> parse -> filter -> merge-baselines -> dedupe/sort
  - [x] Subtask 8.3: `static_fallback_models(id)` per provider (each includes its `default_model()`)
- [x] Task 9: Per-backend endpoint + parse + filter (`src/provider/{openai,groq,anthropic,gemini,ollama}.rs`)
  - [x] Subtask 9.1: `pub(crate)` models-endpoint URL builder per backend (reusing each `base_url()`/`API_KEY_ENV`; Gemini uses `x-goog-api-key` header at `/v1beta/models`)
  - [x] Subtask 9.2: `pub(crate)` parse fn per response shape (`data[].id`; Gemini `models[].name` strip `models/`; Ollama `models[].name`)
  - [x] Subtask 9.3: `keep_chat_model` per provider (OpenAI/Groq exclude-list whisper/tts/dall-e/embedding/moderation/guard; Gemini `supportedGenerationMethods ∋ generateContent`; Anthropic/Ollama passthrough) — pt 2
  - [x] Subtask 9.4: static `*_FALLBACK_MODELS` per backend
- [x] Task 10: Re-exports (`src/provider/mod.rs`)
  - [x] Subtask 10.1: `mod models;` + re-export `fetch_supported_models` / `ModelFetchOutcome`
- [x] Task 11: Phase 2 unit tests (network-free, `&str` bodies / `Vec` inputs)
  - [x] Subtask 11.1: each provider sample JSON -> ids; filter drops whisper/tts/dall-e/embedding, keeps `gpt-*`/`claude-*`/Gemini `generateContent`
  - [x] Subtask 11.2: no-key -> fallback (no network); failure/empty -> fallback; merged list ⊇ `{default_model(), current default}`; `source`/`warning` set correctly

### Phase 3: CLI + cliclack wizard — D1, D5

- [ ] Task 12: Dependencies (`Cargo.toml`)
  - [ ] Subtask 12.1: Add `cliclack = { version = "0.5", default-features = false }` + `console = { version = "0.16", default-features = false, features = ["std"] }`
  - [ ] Subtask 12.2: Record the `cargo tree` delta (lean-binary check) for the PR
- [ ] Task 13: CLI surface (`src/cli.rs`)
  - [ ] Subtask 13.1: Add `Provider` variant to `Commands` (doc-comment mirroring `Config`/`Status`) + a parse test
- [ ] Task 14: Dispatch (`src/main.rs`)
  - [ ] Subtask 14.1: `run_provider_subcommand()` dispatched early in `run()` (before repo/onboarding), non-TTY guard -> guidance + exit 1 (mirror `gcm config`)
- [ ] Task 15: Wizard module (`src/provider/wizard.rs` NEW)
  - [ ] Subtask 15.1: Pure helpers: `build_provider_config(...)` (default ∈ enabled, >=1 enabled), list assembly; reuse shared provider label/order/token helpers (see Task 18)
  - [ ] Subtask 15.2: cliclack flow: intro chip -> provider `select(.filter_mode)` -> credential/endpoint resolution (env > config > `password` prompt, before fetch, no echo) -> spinner over `fetch_supported_models` (uses `ModelFetchOutcome`) -> `multiselect(.filter_mode)` >=1 -> default `select` -> `merge_provider_config` + `config::save` -> outro
  - [ ] Subtask 15.3: Cancellation (Esc/Ctrl-C) -> `outro_cancel` + non-zero exit, no partial write, config untouched (pt 16)
- [ ] Task 16: Phase 3 unit tests for the pure assembly/validation helpers in `wizard.rs`

### Phase 4: Integration tests, docs, polish

- [ ] Task 17: Acceptance tests (`tests/provider.rs` NEW, subprocess + `GCM_CONFIG` + `stdin` null, mirror `tests/onboarding.rs`, no network)
  - [ ] Subtask 17.1: `gcm provider` non-TTY -> exit != 0 + guidance on stderr
  - [ ] Subtask 17.2: a saved v1 config hydrates after the bump (not `OnboardingRequired`)
  - [ ] Subtask 17.3: `models = ["a"]` + `--model b` -> error code `Config` (names `b` + lists set); empty `models` allows free-form `--model`
  - [ ] Subtask 17.4: enforcement on a clean/no-op repo errors (intentional, pt 13)
  - [ ] Subtask 17.5: `gcm config` after a `gcm provider` whitelist preserves `models` (pt 2)
- [ ] Task 18: Module-visibility cleanup (pt 14)
  - [ ] Subtask 18.1: Lift shared provider label/order/token helpers (`cloud_then_ollama`, `provider_label`, `provider_token`) to `src/provider/mod.rs` (single source) or mark needed `config.rs` helpers `pub(crate)`; no duplication
- [ ] Task 19: Documentation
  - [ ] Subtask 19.1: README — document `gcm provider` (flow, enabled-models whitelist, env interplay)
  - [ ] Subtask 19.2: Confirm `commented_reference`/templates reflect `models`
- [ ] Task 20: Deferred middle-severity items (apply if cheap during impl, else leave noted)
  - [ ] Subtask 20.1: `.max_rows(~20)` on the model multiselect (pt 4)
  - [ ] Subtask 20.2: concrete fast-fetch transport tuning already in Task 7.2 (pt 11); confirm values (3-5s, 0-1 retries)

### Phase 5: Testing & Validation

- [ ] Task 21: `cargo fmt --check` clean
- [ ] Task 22: `cargo clippy --all-targets -- -D warnings` clean
- [ ] Task 23: `cargo test` full suite green (existing + new unit + `tests/provider.rs`)
- [ ] Task 24: Manual TTY verification of the cliclack flow against a large list (Ollama/OpenAI) — record screenshot/recording for the PR (covers AC-1, AC-3)
- [ ] Task 25: Codex + (best-effort) Gemini pre-PR validation gate; address findings

### Phase 6: Finalization

- [ ] Task 26: Create PR
  - [ ] Subtask 26.1: Conventional commits `feat(CLO-516): …` on `feat/clo-516-providers`
  - [ ] Subtask 26.2: Push branch: `git push -u origin feat/clo-516-providers`
  - [ ] Subtask 26.3: `gh pr create` with body linking CLO-516 + the design doc + the manual-verify recording
  - [ ] Subtask 26.4: Confirm CI green

---

## Module Structure

- `src/config.rs` - `ProviderConfig.models`, version bump + migration, `model_is_enabled` + `canonicalize_model`, `merge_provider_config`, `run_wizard` preservation, render/template updates (Modified)
- `src/provider/models.rs` - `fetch_supported_models` + `ModelFetchOutcome` + `static_fallback_models` (New)
- `src/provider/http.rs` - `get_json` GET sibling (Modified)
- `src/provider/wizard.rs` - cliclack flow + pure assembly (New)
- `src/provider/mod.rs` - re-exports + lifted shared helpers (Modified)
- `src/provider/{openai,groq,anthropic,gemini,ollama}.rs` - endpoint/parse/filter/fallback (Modified)
- `src/cli.rs` - `Commands::Provider` (Modified)
- `src/main.rs` - dispatch + enforcement hook (Modified)
- `Cargo.toml` - `cliclack`, `console` (Modified)
- `tests/provider.rs` - acceptance tests (New)
- `README.md` - `gcm provider` docs (Modified)

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. Recalculate the percentage from completed top-level tasks.

---

## Notes

- Build Phase 1 (pure config) fully green before Phase 2; Phase 2 green before the TUI in Phase 3.
- Keep cliclack in the thin `wizard.rs` shell; everything testable is a pure fn (the TUI itself is manual-verify only — it reads `/dev/tty`).
- Hard constraints (design Constraints): sync only (no tokio/async), atomic `0600` writes, never echo a key (Gemini key in `x-goog-api-key` header, never URL), no partial config on cancel, preserve other providers + existing whitelists.
- Acceptance criteria AC-1..AC-9 and eval rows 1-17 live in the design doc; map tests back to them.
- Open questions Q1-Q3 are intentional deferrals (delegate `gcm config` to the wizard later; spinner fetch budget; cache fetched lists).
