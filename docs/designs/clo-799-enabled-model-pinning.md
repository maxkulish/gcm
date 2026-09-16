# Design: CLO-799 — Fix the enabled-model set pinning gcm to one model

## Problem

A `google` config that was once saved by `gcm provider` with a single enabled model
(`models = ["gemini-3.1-flash-lite"]`) can never reach a newer Gemini. Three surfaces
fail closed (see `docs/discovery/clo-799.md`):

1. `enforce_enabled_model` (`src/main.rs:301-307`) rejects `--model gemini-3.5-flash-lite`
   with exit 1, and the message lists only the *enabled* entry.
2. The wizard's multiselect pre-selects the current enabled set; a filtered view hides
   the pre-selected row, so Enter re-saves the same single entry. The error message's only
   remediation is the same wizard, so the trap is self-perpetuating.
3. `initial_default_model` falls back to `selected.first()` instead of
   `ProviderId::default_model()`, so the wizard's default can disagree with the shipped
   Google default (`gemini-3.5-flash-lite`).

This design keeps the whitelist gate (CLO-516 D4/AC-6 is an intentional, tested contract -
the issue's AC2 only asks for a better message) and fixes all three surfaces.

## Goals / Non-goals

**Goals**

- AC1: a user on a 3.1-only config can enable `gemini-3.5-flash-lite` through
  `gcm provider` (no hand-editing `config.toml`) and then run it via `--model`.
- AC2: the enabled-set rejection message names the models that are available, not only
  the enabled ones.
- AC3: `ProviderId::Google.default_model()` and the wizard's default pre-highlight agree.
- Preserve CLO-516 D4/AC-6: a non-empty enabled set still rejects `--model`, env, and
  config values outside it.
- No config-schema change, no network call on the rejection path, no public library
  surface change.

**Non-goals**

- Letting `--model` bypass the enabled set (considered and rejected; see below).
- Adding an interactive auto-repair prompt that mutates `config.toml` on rejection.
- Re-fetching the live catalog in the rejection message.
- Reworking `gcm provider` into a different selection widget.

## Architecture

Three independent, small changes; no data-flow change.

### A. Rejection message carries the provider's known catalog

`model_is_enabled` (`src/config/mod.rs:533`) is the pure decision point. It currently
returns a message naming the offender and the enabled set only. Enrich it with the
provider's built-in discovery catalog, minus entries already enabled.

- `src/provider/models.rs`: change `fn static_fallback_models` to
  `pub(crate) fn static_fallback_models` (crate-internal only - **not** a public item, so
  the locked library surface is unchanged).
- `src/config/mod.rs::model_is_enabled`: on rejection, compute the known-but-not-enabled
  catalog entries (canonical comparison, same rule as membership) and append them to the
  message:

  ```
  model '{model}' is not enabled for {provider}. Enabled: {enabled}.
  Known {provider} models: {known}. Run `gcm provider` to enable it, or clear the list
  to allow any.
  ```

  If `known` is empty (e.g. Ollama, whose catalog is empty), omit the "Known …" clause.
  The known-vs-enabled diff uses the same `canonicalize_model(id, …)` comparison as the
  membership check on both sides (so an enabled `models/gemini-x` is not re-listed as
  known), and the rendered list is capped at 8 entries with a trailing `…` when longer
  (today the largest catalog is 6 short ids, so the cap is future-proofing, not a
  user-visible change). The message stays a single `String`; the call site in `main.rs`
  is unchanged.

This is deterministic and offline: `static_fallback_models` does no I/O and compiles
without the `cli` feature. The message also keeps the actionable `gcm provider` pointer.

### B. Wizard makes the SPACE/ENTER contract unmissable

`run_provider_wizard` (`src/config/facade.rs:184-268`):

- Emit a `cliclack::note` immediately before the multiselect:

  > **Enable models** — Type to filter. Press **SPACE** to toggle a model on/off, then
  > **ENTER** to save the checked set. Filtering alone does **not** change the selection.

- Make the prompt explicit:
  `"Enable models (SPACE toggles; ENTER saves the checked set)"`.

The pre-selection stays (`initial_values(initial_enabled)`), preserving CLO-516 AC-5
(re-run pre-selects the current set). The note addresses the trap directly: when a filter
hides a pre-selected row, the user is told that the selection is unchanged by filtering.

### C. Wizard default agrees with the shipped default

- `wizard_model_list` (`src/config/mod.rs:653`) additionally unions
  `id.default_model()` (canonical dedupe), so the shipped default is always a candidate
  even when a successful live fetch omits it. This partially realises CLO-516 D7.3's
  "always merge the provider baseline" intent for the default.
- `initial_default_model` (`src/config/mod.rs:703`) gains a middle preference:
  1. the current default, if it survived into `selected`;
  2. else `id.default_model()`, if it is in `selected`;
  3. else `selected.first()`.

  The return type is unchanged (`Option<String>`); on an empty `selected`,
  `selected.first().cloned()` yields `None` and no panic path exists (the wizard's
  `.required(true)` prevents an empty submit anyway). So a fresh wizard run that
  includes `gemini-3.5-flash-lite` pre-highlights it, and a user who deliberately
  selects only `gemini-3.1-flash-lite` still gets 3.1.

### Why not let `--model` override the gate

`docs/designs/clo-516-interactive-provider-wizard.md` D4/AC-6 deliberately includes
`--model` in the enforced sources, and `tests/provider.rs`
(`enabled_model_outside_set_is_rejected`, `enforcement_runs_on_clean_repo`) encodes it.
CLO-799's AC2 explicitly keeps the rejection and only requires the message to name the
available models. Reversing the contract would weaken the non-chat-model guard (a typo'd
embedding model would reach the provider) and ripple through two tests, the CLO-516 design
doc, and the README for a convenience the wizard now provides. Rejected.

## Public API surface

No public library API changes. `static_fallback_models` moves from private to
`pub(crate)`; it stays absent from `cargo doc --lib`. The modified helpers
(`model_is_enabled`, `wizard_model_list`, `initial_default_model`) are already
`#[doc(hidden)] pub` and keep their signatures.

## Assumptions

| # | Assumption | Confidence | Verification |
|---|---|---|---|
| A1 | The built-in static catalog is an adequate "available models" list for the rejection message; a live fetch on the error path is unnecessary. | medium | Unit test asserts a non-enabled catalog entry appears in the message; the message points at `gcm provider` for the live catalog. |
| A2 | `static_fallback_models` is transport-free and available without the `cli` feature, so `config` may call it in the pure helper. | high | `cargo build --no-default-features --lib` succeeds; the function body has no `http`/`cli` reference. |
| A3 | `cliclack::note(prompt, message)` renders an always-visible bordered message in cliclack 0.5 and is safe on the TTY-only wizard. | high | cliclack 0.5.4 `lib.rs:427` exposes `note`; `gcm provider` already guards on an interactive TTY. |
| A4 | Preserving the gate still satisfies AC1 because `gcm provider` is the documented non-hand-edit path, and once enabled a model passes `--model`. | medium | `tests/provider.rs`: a config enabling both 3.1 and 3.5 lets `--model gemini-3.5-flash-lite` pass enforcement (fails later at a closed endpoint, not `Config`). |
| A5 | Unioning `id.default_model()` into the candidate list does not change which models are pre-selected. | high | `initial_enabled` filters candidates by canonical match against `current_enabled`; the default is not in `current_enabled` on a fresh run. Unit test on `wizard_model_list`. |
| A6 | `initial_default_model`'s new middle preference does not regress the current-default case. | high | Existing test extended: current default wins; shipped default second; first selected last. |
| A7 | No config migration is required: the schema, format version, and persisted fields are unchanged. | high | `git diff` touches no `ProviderConfig` field; `cargo test` v1/v2 migration tests stay green. |

## Test plan

**Unit (`src/config/mod.rs`)**

- `wizard_model_list_includes_provider_default` - Openai list always contains
  `gpt-5.6-terra` even when `fetched` omits it; canonical dedupe holds.
- `initial_default_model_prefers_shipped_default` - with no current default and a selected
  set containing the shipped default, it is chosen; a set without it falls back to
  `selected.first()`.
- `model_is_enabled_message_lists_known_catalog` - Openai enabled `["gpt-5.6-terra"]`,
  offending `dall-e-3`; message contains `gpt-5.6-luna` (known, not enabled) and still
  contains the offender and `gcm provider`.

**Integration (`tests/provider.rs`)**

- Extend `enabled_model_outside_set_is_rejected` to assert the message names a known
  catalog model (`gpt-5.6-luna`).
- New `newly_enabled_model_passes_enforcement` - config `models = ["gemini-3.1-flash-lite",
  "gemini-3.5-flash-lite"]`, `--model gemini-3.5-flash-lite`, closed base URL; assert the
  error is **not** `Config` (enforcement passed; the run proceeds to the provider and
  fails at transport). Mirrors the `empty_models_allows_any_model` technique.

**Manual**

- On a 3.1-only config, run `gcm provider`; confirm the note is visible, that filtering to
  `3.5`, pressing SPACE, then ENTER enables `gemini-3.5-flash-lite`, and that the
  default-select pre-highlights the shipped default when it is enabled.

**Pre-merge gate**

- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
  (plus `cargo build --no-default-features --lib` for the library boundary).

## Migration / rollout

No migration. The config format version stays 2 and no field is added or renamed. The
change is additive UX: a longer error message, an extra wizard note, and a changed default
pre-highlight. Existing configs load and enforce exactly as before. README's
`Selecting which models to use` section gets one clarifying sentence about filtering not
changing the selection.

## Open questions

- Should the rejection message eventually fetch the live catalog (network) instead of the
  static one? Deferred: offline determinism and zero latency on the error path win; the
  message names the static catalog and points to `gcm provider` for the live list.
- Should `gcm provider` default-enable the shipped default on a fresh run (so a user who
  only presses Enter gets it)? Out of scope here; the wizard requires an explicit first
  selection, and this design only fixes which selection is pre-highlighted.
