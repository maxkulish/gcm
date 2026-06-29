# Status output readability: regroup the Providers section

- Date: 2026-06-29
- Status: **implemented** (2026-06-29). `cargo test` / `clippy` / `fmt` green.
- Affected component: `src/status.rs` (`print_human` + new `locality_tag` / `print_provider_section` / `print_provider_block` helpers) - human output only. Plus a one-line correctness fix to cloud-model detection shared with `src/provider/mod.rs` (see "Cloud/local detection").
- Scope: mostly cosmetic / UX. No change to attribution logic or the `--json` payload (`StatusReport` and all helpers stay byte-identical). The one behavior change is the cloud-detection broadening below, which makes the privacy/egress signal more accurate (errs toward flagging cloud).
- Decision: chosen from a 3-option comparison (aligned table / active banner + list / de-noised nested). **De-noised nested** won: smallest change from today, width-robust, keeps the familiar `key:` / `model:` blocks.

## Problem

`gcm status` human output is hard to scan:

1. **No headline answer.** The one fact a user opens `status` for - "what runs when I type `gcm`" - is buried as a `[selected, activated]` tag on whichever provider happens to be selected (often last). The eye has to hunt.
2. **Tag noise.** `[activated]` / `[not activated]` / `[selected, activated]` repeat on every block and carry little signal.
3. **No grouping.** Activated and not-activated providers interleave in canonical order, so a dead provider (e.g. `anthropic`, no key) sits between live ones.

### Before

```
gcm 0.1.6+206b8a5

Paths:
  config dir source: default dir
  config dir:        /Users/mk/.config/gcm
  config file:       /Users/mk/.config/gcm/config.toml (exists)

Providers:
  groq [activated]
    key:   env var GROQ_API_KEY
    model: openai/gpt-oss-120b (default)
  google [activated]
    key:   env var GEMINI_API_KEY
    model: gemini-3.1-flash-lite (config file)
  openai [activated]
    key:   env var OPENAI_API_KEY
    model: gpt-5.4-mini (default)
  anthropic [not activated]
    key:   not set
    model: claude-haiku-4-5 (default)
  ollama [selected, activated]
    endpoint: http://localhost:11434 (default)
    model: nemotron-3-nano:30b-cloud (config file)
```

## After (chosen design)

`version` and the whole `Paths:` block are unchanged. Only the `Providers:` block is replaced by a `Selected` headline plus two grouped sections.

```
gcm 0.1.6+206b8a5

Paths:
  config dir source: default dir
  config dir:        /Users/mk/.config/gcm
  config file:       /Users/mk/.config/gcm/config.toml (exists)

Selected (gcm will use this):
  ollama -> nemotron-3-nano:30b-cloud (config file) [cloud]

Activated:
> ollama
    endpoint: http://localhost:11434 (default)
    model:    nemotron-3-nano:30b-cloud (config file) [cloud]
  groq
    key:   env var GROQ_API_KEY
    model: openai/gpt-oss-120b (default)
  google
    key:   env var GEMINI_API_KEY
    model: gemini-3.1-flash-lite (config file)
  openai
    key:   env var OPENAI_API_KEY
    model: gpt-5.4-mini (default)

Not activated:
  anthropic
    key:   not set
    model: claude-haiku-4-5 (default)
```

### Glyphs

The reviewed mockup used `→` (U+2192) for the selected marker. The existing code is ASCII-only, so this spec writes the ASCII forms `->` / `>`. Either style is acceptable; if Unicode is chosen, use `→` for both the headline arrow and the section marker. Do not mix. The Ollama cloud/local descriptor is a plain `[cloud]` / `[local]` tag in both styles.

## Rendering rules

The block order is: `Selected` headline -> (existing `Warning:` line, if any) -> `Activated:` -> `Not activated:`. Keep the `Warning: <provider_error>` line exactly where it is today (printed when `GCM_PROVIDER` is unknown); it stays between the headline and the sections.

### Selected headline

A `Selected (gcm will use this):` heading, then one indented line:

```
  <provider> -> <model> (<model_source>)
```

Where `<provider>` is the `selected` provider, `<model>` / `<model_source>` are that provider's resolved `model` / `model_source`. Then, conditionally:

- **Ollama cloud/local tag.** If the selected provider is `ollama`, append a neutral ` [cloud]` (when `zero_egress == Some(false)`) or ` [local]` (when `zero_egress == Some(true)`) to the model. This reuses the existing `zero_egress` field as a plain descriptor - not a warning. Cloud providers (groq/google/openai/anthropic) get no such tag; the local-vs-cloud distinction is only meaningful for Ollama.
- **Runtime caveat (provider-aware, must be truthful).** The suffix must reflect what the *next* run actually does, so it branches on the provider kind rather than `activated` alone:
  - **Invalid `GCM_PROVIDER`** (`provider_error` is set): the runtime rejects the unknown provider *before* selecting anything, so the next run fails outright. The whole headline is replaced with `  (none - GCM_PROVIDER is invalid; the next run would fail - see Warning below)`. It must NOT name the groq display fallback as what runs (especially dangerous when `GROQ_API_KEY` is set, which would otherwise suppress any caveat).
  - **Cloud provider, not activated** (`!activated` and no `endpoint`, i.e. no API key): the run genuinely errors. Append ` (NOT activated - no API key; gcm would error on a real run)`.
  - **Ollama, not activated** (`!activated` and `endpoint.is_some()`): Ollama is key-free and the endpoint falls back to the default local daemon, so the run can succeed against a running daemon. Do NOT claim an error - append ` (not configured - will try the local Ollama daemon at <endpoint>)`.
  - **Activated:** no suffix.

### Cloud/local detection (correctness fix)

The `[cloud]` / `[local]` tag is driven by `ProviderStatus.zero_egress`, which is computed in `build_report`. Previously both that computation and the runtime egress note (`src/provider/mod.rs`) keyed off `model.ends_with(":cloud")` only - so an Ollama Cloud model named with the `-cloud` tag form (e.g. `nemotron-3-nano:30b-cloud`) was misclassified as **local / zero-egress** in both places. For a privacy signal that is the dangerous direction (claims data stays on-machine when it leaves).

The fix introduces one shared predicate, `ollama::is_cloud_model(&model)`, that returns true for either a `:cloud` or a `-cloud` suffix, and routes both `build_report` and the runtime egress note through it. The two can no longer disagree, and `-cloud` models now correctly read `[cloud]`. This is the only behavior change in the task and it strictly widens what counts as cloud.

### Activated / Not activated sections

- Partition the five providers by `activated`.
- **Order within each section:** the `selected` provider first, then the remaining providers of that section in the existing canonical `PROVIDER_ORDER` (groq, google, openai, anthropic, ollama). So the selected provider leads its own section regardless of canonical position.
- **Per-provider block is the de-noised current block:** the provider name on its own line (no `[...]` tags), then the same indented detail lines as today:
  - cloud providers: `    key:   <key_source>` then `    model: <model> (<model_source>)`
  - ollama: `    endpoint: <endpoint> (<endpoint_source>)` then `    model:    <model> (<model_source>) [cloud|local]` (note `model:` padded to align its value under `endpoint:`; the `[cloud]` / `[local]` tag matches the headline)
- **Selected marker:** the selected provider's name line is prefixed with `> ` (or `→ `); every other provider's name line is prefixed with two spaces so names align. The marker appears in whichever section the selected provider lands in (normally `Activated`, but `Not activated` otherwise).
- **Empty section:** if a section has no providers, print the heading followed by an indented `(none)` line. On a clean machine with no keys, every provider is under `Not activated` and the groq fallback leads it, marked.

## Edge cases

- **Invalid `GCM_PROVIDER`.** `provider_error` is set and `selected` falls back to groq (unchanged logic). The headline is the `(none - ... would fail ...)` form above (it does *not* assert groq); the `Warning:` line still prints below and explains the groq display fallback. The sections still mark groq with `>` as that fallback - coherent because the Warning names it explicitly.
- **No config dir / malformed config.** Paths block already handles these; providers fall back to env-derived state. Sections render normally from whatever activation the env yields.
- **Detail-line values for not-activated providers** (e.g. `key: not set`, default model) are kept - they show what the provider *would* use once activated, which is useful and costs nothing.

## Implementation notes (as built)

- `print_human` in `src/status.rs` was rewritten; the per-provider rendering is factored into `print_provider_block` (one block, takes the `>`/space marker via `selected`) and `print_provider_section` (filter by `activated`, stable-sort `!selected` to float the selection, `(none)` when empty). `locality_tag` maps `zero_egress` to the `cloud`/`local` string. No `StatusReport` struct changes.
- `ollama::is_cloud_model` (new `pub(crate)` predicate in `src/provider/ollama.rs`) is the single source of truth for cloud detection; `build_report` (`src/status.rs`) and the egress note (`src/provider/mod.rs`) both call it.
- The `--json` path (`serde_json::to_string(&report)`) is untouched - machine consumers see no change.

## Test impact

- `tests/status.rs::status_model_flag_scoped_to_selected_provider` was updated: the old `openai [selected` assertion is replaced by checks for the `Selected (gcm will use this):` headline, the `> openai` marker, and the absence of `[selected` / `[activated]`. `model: custom-model (flag)` is preserved.
- New `tests/status.rs::status_human_layout_groups_and_cloud_tag`: a `-cloud` Ollama model as `config.default` renders `ollama -> nemotron-3-nano:30b-cloud (config file) [cloud]`, both section headings appear, the selected provider carries `>`, and `not zero-egress` framing is absent.
- New unit `src/provider/ollama.rs::is_cloud_model_detects_both_suffixes`; `src/status.rs::ollama_zero_egress_flag` extended with a `-cloud` case.
- New `tests/status.rs::status_invalid_gcm_provider_headline_does_not_claim_use`: with `GCM_PROVIDER=bogus` + `GROQ_API_KEY` set, the headline reads `(none - ... the next run would fail ...)`, never `groq ->`, and the `bogus` Warning still prints.
- New `tests/status.rs::status_ollama_selected_unconfigured_does_not_claim_error`: `--provider ollama` on a clean machine names ollama in the headline with the `will try the local Ollama daemon` note and no `would error on a real run` claim.
- `status_no_config_clean_env_exits_zero`, `status_env_key_and_model_attribution`, `status_ollama_endpoint_source` pass unchanged (paths + detail lines byte-identical).

## Acceptance criteria

- [x] `version` line and the entire `Paths:` block are byte-identical to before.
- [x] A `Selected (gcm will use this):` headline names the provider + model + source on its own line.
- [x] The Ollama model carries a neutral `[cloud]` / `[local]` tag (driven by `zero_egress`); no warning/egress framing remains.
- [x] Providers are split into `Activated:` and `Not activated:` sections; the selected provider leads its section and is marked.
- [x] `[activated]` / `[not activated]` / `[selected, ...]` tags are gone.
- [x] The runtime caveat is truthful: invalid `GCM_PROVIDER` shows the run will fail (not the groq fallback); an unconfigured cloud provider shows it errors; an unconfigured Ollama shows the local-daemon fallback, not an error.
- [x] Cloud detection recognizes both `:cloud` and `-cloud` (shared `ollama::is_cloud_model`), consistent across status and the runtime egress note.
- [x] `gcm status --json` output is unchanged.
- [x] `cargo test` green (316 tests), `clippy` clean, `fmt` clean.
