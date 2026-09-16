# Plan: CLO-799 Fix the enabled-model set pinning gcm to one model

## Context
- Design: docs/designs/clo-799-enabled-model-pinning.md
- Discovery: docs/discovery/clo-799.md
- Linear: https://linear.app/cloud-ai/issue/CLO-799/fix-the-enabled-model-set-pinning-gcm-to-one-model-so-a-newer-gemini

Prior lessons consulted (`.pi/lessons/`):
- `clo-598-consumer-check-lessons.md § L2` - tests must execute the branch they claim;
  the new integration test drives the real enforcement path, not just the message string.
- `clo-493-...-lessons.md § L1` - keep `--json` stdout machine-readable; the rejection
  message stays in the error envelope, never on stdout prose.
- `clo-594-library-boundary-lessons.md § L1/L3` - the binary imports from `gcm::`, so
  helpers stay in the library; `static_fallback_models` becomes `pub(crate)`, not `pub`,
  to keep the locked public surface unchanged.

## Sub-tasks

### ST1 Enrich the enabled-set rejection message with the known catalog
**Files:** `src/provider/models.rs`, `src/config/mod.rs`
**Acceptance:** `cargo test config::tests::model_is_enabled_message_lists_known_catalog` passes
**Estimate:** S

- `static_fallback_models` → `pub(crate)`.
- `model_is_enabled`: on rejection, diff `static_fallback_models(id)` against the enabled
  set via `canonicalize_model` on both sides; append `Known {provider} models: …` (capped
  at 8 entries + `…`), omitted when the catalog is empty.
- Unit test: Openai enabled `["gpt-5.6-terra"]`, offending `dall-e-3` → message contains
  `dall-e-3`, `gpt-5.6-luna`, `gpt-5.6-terra`, and `gcm provider`.

### ST2 Make the wizard's SPACE/ENTER contract unmissable
**Files:** `src/config/facade.rs`
**Acceptance:** `cargo clippy --all-targets -- -D warnings` is green (wizard is TTY-only; the
note + prompt are exercised manually per the design's Manual test plan)
**Estimate:** S

- Add a `cliclack::note` before the multiselect stating SPACE toggles / ENTER saves the
  checked set and that filtering does not change the selection.
- Change the multiselect prompt to
  `"Enable models (SPACE toggles; ENTER saves the checked set)"`.

### ST3 Make the wizard default agree with `ProviderId::default_model()`
**Files:** `src/config/mod.rs`
**Acceptance:** `cargo test config::tests::wizard_model_list config::tests::initial_default_model` passes
**Estimate:** S

- `wizard_model_list`: union `id.default_model()` (canonical dedupe).
- `initial_default_model`: current default → `id.default_model()` if selected →
  `selected.first()`.
- Update `wizard_model_list_unions_fetched_enabled_and_default` and
  `initial_default_model_prefers_current_then_first` for the new ordering/middle preference.

### ST4 Integration coverage + README clarification
**Files:** `tests/provider.rs`, `README.md`
**Acceptance:** `cargo test --test provider` passes
**Estimate:** S

- Extend `enabled_model_outside_set_is_rejected` to assert the message names a known
  catalog model (`gpt-5.6-luna`).
- Add `newly_enabled_model_passes_enforcement`: config enables both `gemini-3.1-flash-lite`
  and `gemini-3.5-flash-lite`, `--model gemini-3.5-flash-lite` with a closed base URL →
  error code is **not** `Config` (enforcement passed; fails later at transport).
- README `Selecting which models to use`: one sentence that filtering alone does not change
  the selection (SPACE toggles, ENTER saves).

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- Superset: `cargo build --no-default-features --lib` (library boundary stays transport-free).

## Risks
- **Contract drift**: changing `wizard_model_list` ordering could alter pre-selection.
  Mitigated by A5 (the default is not in `current_enabled` on a fresh run) and the updated
  unit tests.
- **Integration test flakiness**: `newly_enabled_model_passes_enforcement` must not hit the
  network; it mirrors `empty_models_allows_any_model`'s closed-endpoint + zero-retry setup.
- **Message size**: catalogs are small (≤6 ids); the 8-entry cap is future-proofing.
