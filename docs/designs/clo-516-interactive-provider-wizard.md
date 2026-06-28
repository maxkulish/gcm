# CLO-516: Add interactive `gcm provider` setting with cliclack (Goose-style provider/model picker)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-516/add-interactive-gcm-provider-setting-with-cliclack-goose-style
**Status**: Finalized
**Author**: Max Kulish
**Created**: 2026-06-28
**Finalized**: 2026-06-28
**Approved By**: Max Kulish (owner, after 2 rounds of implementation-level review)
**Workflow**: `/task:orchestrate` (development, discovery skipped — ticket is spec-grade)

---

## Summary

Add a new `gcm provider` subcommand that opens a polished, Goose-style interactive wizard (via the `cliclack` crate) to: pick one provider from the predefined set, fetch that provider's live model list (with a static fallback), multiselect-with-filter which models to enable, choose exactly one default among them, and persist the result. This introduces a per-provider **enabled-models whitelist** to the config schema (a versioned migration) and enforces it at runtime so only enabled models can be used.

It is additive to the existing minimal onboarding wizard (CLO-496) and the read-only `gcm status` command (CLO-515): `gcm provider` is the new *model-aware* configuration surface, distinct from first-run onboarding which intentionally stays minimal (ADR-001 Decision 11).

---

## Background

`gcm` already has a provider/config story from earlier slices:

- **Provider trait + registry** (CLO-489, `src/provider/mod.rs`): a *synchronous* `Provider` trait (ADR-001 Decision 2 — blocking client, no async), a `ProviderId` enum (Groq default, Google, OpenAI, Anthropic, Ollama), `select()` constructing a model-bound `Box<dyn Provider>`, and `resolve_model_with_source` (precedence flag > per-provider env > default). Models are **free-form strings today**, validated only by the provider's API at call time.
- **Config + onboarding wizard** (CLO-496, `src/config.rs`): a versioned `0600` TOML `config.toml` (`Config { version, default, providers: Vec<ProviderConfig> }`, `ProviderConfig { id, key, endpoint, model }`), atomic writes, a minimal stdin/`stty` first-run wizard (`run_wizard`), and an `apply_to_env` bridge preserving `flag > env > config > default`.
- **Status introspection** (CLO-515, `src/status.rs`): read-only `gcm status` reporting active provider/model/source.
- **CLI** (`src/cli.rs`): clap-derive `Commands { Config, Status }`, dispatched in `src/main.rs` `run()`.

What's missing, and what CLO-516 adds: there is **no curated model selection**. The user cannot discover a provider's available models, cannot restrict usage to a chosen subset, and the only interactive setup is the bare-bones onboarding prompts. Goose (also Rust) solves the same problem with `cliclack` — reusing that exact crate gives the identical diamond-marker UI, filterable lists, and spinner for free. The model multiselect is a deliberate divergence from Goose (which single-selects the model): gcm lets the user enable 1..N models and then pick one default.

### Prior research (from the ticket)

- Goose reference (both projects are Rust): `crates/goose-cli/src/commands/configure.rs` (prompt flow), `crates/goose-providers/src/base.rs` + `openai.rs`/`anthropic.rs` (`fetch_supported_models` live `v1/models` GET + `*_KNOWN_MODELS` static fallback). Crate pins: `cliclack = "0.5"`, `console = "0.16"`.
- Filter: cliclack `0.5` `.filter_mode()` is **fuzzy-ish ranking** (verified — `strsim::jaro_winkler` + a substring-containment bonus, weak matches dropped), *not* plain substring (correction to an earlier draft, review pt 5). Practically: "type the first letters" works, exact substrings stay visible via the contains bonus, results may include non-substring fuzzy matches, and a literal space is awkward in the multiselect (space also toggles). Good enough for v1; `nucleo-matcher`/`fuzzy-matcher` only if it disappoints. (Large lists also want `.max_rows(~20)` — deferred, see below.)

---

## Architecture

### Component overview

```
gcm provider  (new subcommand)
   │
   ├─ src/cli.rs        Commands::Provider variant
   ├─ src/main.rs       run() dispatch → run_provider_subcommand()
   │
   └─ src/provider/wizard.rs            (NEW) cliclack IO shell + pure assembly
         │   intro/select/spinner/multiselect(.filter_mode)/select/outro
         │
         ├─ src/provider/models.rs      (NEW) fetch_supported_models(id, key, endpoint)
         │       per-provider GET + JSON parse + static fallback   (sync, ureq)
         │       └─ reuses src/provider/http.rs (new get_json sibling of post_json)
         │
         └─ src/config.rs               ProviderConfig.models: Vec<String> (NEW field)
                 version 1 → 2 migration; render/load; atomic 0600 write
                 runtime enforcement helper (model ∈ enabled set)
```

The hot commit path (`select` → `generate_plan`/`generate_message`) is **untouched** except for a single new validation call that rejects a resolved model outside a non-empty enabled set.

### Affected components

| Component | Change | Description |
|-----------|--------|-------------|
| `src/cli.rs` | Modified | Add `Provider` variant to `Commands` (doc-comment mirrors `Config`/`Status`). |
| `src/main.rs` | Modified | Dispatch `Commands::Provider` early in `run()` (before repo/onboarding), like `Config`/`Status`; non-TTY guard. **Also** the model-enforcement call in `ensure_configured()` (where the loaded `Config` is still in scope — see D4). |
| `src/provider/wizard.rs` | **New** | cliclack flow (header chip, provider select, spinner, filterable model multiselect, default select, outro) + pure config assembly. |
| `src/provider/models.rs` | **New** | `fetch_supported_models(id, key, endpoint) -> ModelFetchOutcome`; per-provider endpoint/parse + chat-only filter + baseline merge + static fallback. |
| `src/provider/http.rs` | Modified | Add `get_json` (GET sibling of `post_json`, same auth/header/retry machinery). |
| `src/provider/mod.rs` | Modified | Re-export `fetch_supported_models`; add `static_fallback_models(id)`; expose `model_endpoint`/parse hooks per backend as needed (`pub(crate)`). |
| `src/config.rs` | Modified | `ProviderConfig.models: Vec<String>`; bump `CONFIG_FORMAT_VERSION` 1→2 + migration (sets version) in `parse_config`; `render_config` forces v2 (D3, pt 9); `model_is_enabled` + canonicalization (pt 17); `merge_provider_config` (D8, pt 1); `run_wizard`/`build_config` **preserve existing `models`** (pt 2); render/reference/template updates. |
| `src/provider/{groq,gemini,openai,anthropic,ollama}.rs` | Modified | Each exposes its models-endpoint URL + response-shape parse + `keep_chat_model` (or small `pub(crate)` fns) used by `models.rs`; each gets a static `*_FALLBACK_MODELS` incl. its `default_model()`. |
| `Cargo.toml` | Modified | Add `cliclack = { version = "0.5", default-features = false }`, `console = { version = "0.16", default-features = false, features = ["std"] }` (Goose-style flags for the lean-binary posture, pt 15) — confirm the `cargo tree` delta in the PR. |
| `tests/provider.rs` | **New** | Non-TTY guard + migration + enforcement acceptance tests (mirror `tests/onboarding.rs`). |
| `README.md` | Modified | Document `gcm provider`. |

### Dependencies

- **Internal**: `provider::http` (GET transport + retry), `provider::ProviderId` (key/model env, defaults, tokens), `config` (schema, atomic write), `paths` (config dir).
- **External (new)**: `cliclack = "0.5"` (interactive prompts) and `console = "0.16"` (header-chip styling). These pull a moderate terminal-UI subtree (`console`, `crossterm`/termios) into a deliberately lean 7-dep crate. Justification: matches Goose exactly (proven UX, no bespoke TUI code), and the alternative — extending the hand-rolled `stty`/stdin prompts to a filterable multiselect — is substantial, fragile, and worse UX. No `nucleo-matcher`/`fuzzy-matcher` in v1 (substring `.filter_mode()` first). No `tokio` (sync only, ADR-001 Decision 2).

---

## Detailed design

### D1 — `gcm provider` vs `gcm config` / onboarding (relationship)

`gcm provider` is a **new, separate** subcommand — the model-aware configuration surface. It does *not* replace first-run onboarding, which ADR-001 Decision 11 deliberately keeps minimal ("no forced model selection"). Decision:

- `gcm provider` — the new cliclack wizard (this task). Run anytime to (re)configure provider + enabled models + default.
- `gcm config` / first-run onboarding (`run_wizard`) — **unchanged** in this task. They remain the minimal "enable providers + capture keys + pick default" flow. (A later task may delegate `gcm config` to the new wizard; out of scope here to keep the blast radius bounded and avoid regressing the well-tested onboarding path.)

This keeps the migration additive and the existing onboarding tests untouched.

### D2 — `fetch_supported_models`: free function dispatched by `ProviderId` (not a trait method)

The ticket suggests "a new trait method on the provider backends." **Deliberate divergence — owner-ratified at the design checkpoint (2026-06-28).** The `Provider` trait is **model-bound** (constructed by `select()` *with* a chosen model) and lives on the hot commit path. Model-fetching happens *before* a model is chosen and is needed only by the wizard. Adding `fetch_supported_models()` to the core trait would force constructing a provider with a throwaway model and widen the central interface for a wizard-only concern. The per-backend logic still lives *in* each backend module (just as a `pub(crate)` fn, not a trait method), so the "on the provider backends" intent is preserved.

Instead: a free function dispatched by `ProviderId`, in a new `src/provider/models.rs`:

```rust
/// Fetch the provider's available model ids from its live API, or fall back to a
/// static per-provider list on any failure. Never errors out the wizard; returns
/// the source + an optional warning so the caller can message accurately (D7, pt 10).
pub fn fetch_supported_models(
    id: ProviderId,
    key: Option<&str>,         // None for Ollama (key-free) or a cloud provider with no key yet
    endpoint: Option<&str>,    // Ollama / base-URL override
) -> ModelFetchOutcome { /* skip-if-no-key → GET → filter → merge → dedup; on Err → fallback */ }
```

Each backend contributes its endpoint + response shape (a small `pub(crate)` helper per module, reusing its existing `base_url()`/`API_KEY_ENV`). Sync, via `http::get_json`. The live-fetch result is parsed; **any** failure (transport, non-2xx, parse, empty) degrades to the static fallback so the spinner always resolves to a usable list (AC-2). Network calls are bounded by the existing `GCM_HTTP_TIMEOUT_SECS` and made un-retried-or-lightly-retried for snappy UX.

Per-provider endpoints + parse + fallback (base URLs/auth verified against each backend's `base_url()`/`request()`):

| Provider | Endpoint (GET) | Auth | Parse | Chat-only filter (D7.1) | Paginates? |
|----------|----------------|------|-------|--------------------------|------------|
| OpenAI | `{base}/models` (base `…/v1`) | `Bearer` | `data[].id` | **yes** — exclude `whisper`/`tts`/`dall-e`/`*embedding*`/`*moderation*`/`babbage`/`davinci` | no |
| Groq | `{base}/models` (base `…/openai/v1`) | `Bearer` | `data[].id` | **yes** — exclude `whisper`/`distil-whisper`/`tts`/guard models | no |
| Anthropic | `{base}/v1/models?limit=1000` | `x-api-key` + `anthropic-version` | `data[].id` | no (all `claude-*` are chat) | yes (`has_more`/`last_id`) — `limit=1000` mitigates |
| Google/Gemini | `{base}/v1beta/models?pageSize=1000` | `x-goog-api-key` **header** | `models[].name` (strip `models/`) | **yes** — keep only `supportedGenerationMethods ∋ generateContent` | yes (`nextPageToken`) — `pageSize=1000` mitigates |
| Ollama | `{endpoint}/api/tags` | none | `models[].name` | no (locally-pulled) | no |

Each fallback list MUST include that provider's `default_model()` so the default is always selectable offline. (Note the Gemini auth is the `x-goog-api-key` header, matching `gemini.rs`, not a `?key=` query param.)

### D3 — Config schema change + version migration (the load-bearing risk)

Add an enabled-models field; the existing `model` becomes the chosen **default**:

```rust
pub struct ProviderConfig {
    pub id: ProviderId,
    pub key: Option<String>,
    pub endpoint: Option<String>,
    pub model: Option<String>,            // chosen DEFAULT (unchanged meaning)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,              // NEW: enabled set (whitelist)
}
```

**Why bump the version (1 → 2)?** `serde(default)` already lets a v1 file (no `models` key) deserialize. The bump exists for **forward-compat**: a v2 file with a populated `models` whitelist must be rejected by *older* gcm binaries (which would otherwise silently ignore the whitelist and allow disabled models). Today `parse_config` treats any `version != CONFIG_FORMAT_VERSION` as `WrongVersion` → silent miss → re-onboard. So an old binary reading a v2 file safely re-onboards rather than mis-enforcing.

**Migration (mandatory — AC "existing configs load without error"):** `parse_config` must accept **both** version 1 and 2:
- `version == 2` → parse natively.
- `version == 1` → accept and **set `cfg.version = CONFIG_FORMAT_VERSION` in-memory**; leave `models` **empty** (= "unrestricted", see D4). Do **not** auto-populate `models` from the single `model` — that would silently start rejecting a v1 user's free-form `--model`. The whitelist only becomes active once the user explicitly runs `gcm provider`.
- `version` 0 or > 2 → `WrongVersion` (unchanged behavior for genuinely unknown schemas).

**Version-write trap (review pt 9).** `render_config` serializes `config.version` directly, so if migration returned `Config { version: 1, .. }`, a later `save()` would re-write `version = 1` and the bump would never take effect. Two belt-and-suspenders fixes: (a) migration sets `cfg.version = CONFIG_FORMAT_VERSION` (above); **and** (b) `render_config` forces the serialized version to `CONFIG_FORMAT_VERSION` regardless of the in-memory value. Also bump the literal `version = 1` in `sample_toml_template()` and `non_tty_instructions()` to `version = 2`, and add a `models = [...]` example to the commented reference block. Existing unit tests that construct `Config { version: CONFIG_FORMAT_VERSION, .. }` transparently become v2.

### D4 — Runtime enforcement (model ∈ enabled set)

Semantics chosen to satisfy *both* "model outside enabled set is rejected" **and** "existing configs load without error":

- **Empty `models` ⇒ unrestricted** (no whitelist). This is the v1-migration and pre-`gcm provider` state: free-form models work exactly as today. Back-compat preserved.
- **Non-empty `models` ⇒ enforced.** A resolved model (from `--model` flag, per-provider env, or config `model`) that is **not** in the enabled set is rejected with a clear, actionable `ErrorKind::Config`/`GcmError` message naming the offending model and listing the enabled set + how to add it (`gcm provider`).

**Where it hooks in (corrected after tracing the real flow).** `model_is_enabled(cfg, id, model) -> Result<(), String>` is a **pure** helper in `config.rs`, but it canNOT be called from `provider::select()` / `resolve_model` — by that point the `Config` is gone. The real flow is: `main.rs::ensure_configured()` does `config::load()` → `config::apply_to_env(&cfg)` and then **drops `cfg`**; `provider::select()` later resolves the model *purely from `--model` + env vars*. The `models` whitelist is a list, never bridged to env, so it never reaches `select()`.

Therefore enforcement lives in **`main.rs::ensure_configured()` (or `execute()`), while the loaded `cfg` is still in scope**, immediately **after** `apply_to_env`:

```rust
// inside ensure_configured, on the load+apply path (and after onboarding):
config::apply_to_env(&cfg);
let id = provider::pick_provider_id(args.provider, std::env::var("GCM_PROVIDER").ok().as_deref())?;
let (model, _src) = provider::resolve_model_with_source(id, args.model.as_deref(), |v| std::env::var(v).ok());
config::model_is_enabled(&cfg, id, &model).map_err(GcmError::Config)?;   // pure check
```

`pick_provider_id` and `resolve_model_with_source` are already `pub`/`pub(crate)`. Running the check *after* `apply_to_env` means it sees the exact model `select()` will use (config `model` already bridged into the env the resolver reads), so flag/env/config all enforce identically. Env-only / flag-only users (no `cfg` loaded) hit no enabled set → unrestricted, by construction. The wizard guarantees the chosen default `model` ∈ `models`, so a freshly written v2 config always passes its own check.

**Match is exact after per-provider canonicalization (review pt 17).** Enforcement compares the resolved model against the enabled set by exact string equality, but **after** the same canonicalization the fetch/store path uses, so a user isn't falsely rejected: Gemini values are stored/compared with the `models/` prefix stripped; all values are trimmed; **no general case-folding**. For Ollama, a tagless value canonicalizes to `:latest` on both sides (so `--model llama3` matches an enabled `llama3:latest`). Document this in the error message domain so the rule is predictable.

**Enforcement runs before the no-changes check (review pt 13).** `ensure_configured()` fires before the dirty-tree check, so a clean/no-op repo with an out-of-set `GCM_*_MODEL`/`--model` errors rather than returning `noop`. This is **intentional and consistent** with existing onboarding (which also fires pre-check); covered by a `tests/provider.rs` case so it's a deliberate, locked-in choice rather than an accident.

**Trade-off of "empty = unrestricted" (review pt 5).** Because `models` uses `#[serde(default)]` `Vec<String>`, a missing key (v1 migration) and an explicit hand-edited `models = []` both deserialize to an empty vec — indistinguishable — and both mean *unrestricted*. Consequence: a user cannot hand-edit the config to lock a provider to **zero** usable models; clearing the array reverts to free-form. This is intentional: 0-enabled is a degenerate "provider is unusable" state, and the v1→v2 migration *requires* empty-to-mean-unrestricted for back-compat. *Considered and rejected:* `models: Option<Vec<String>>` (`None` = absent/unrestricted, `Some([])` = explicit lock) would distinguish the two, but it complicates every call site to serve a degenerate use case (to disable a provider, remove its `[[providers]]` entry or don't select it as default). The wizard itself never writes `models = []` (it requires ≥1 selected).

### D5 — The cliclack wizard flow (`wizard.rs`)

Thin IO shell over pure helpers (mirrors `config.rs`'s pure/`run_wizard` split, because cliclack reads `/dev/tty` and can't be driven by piped stdin — see Testing):

1. `intro(style(" gcm-provider ").on_cyan().black())` — header chip.
2. `select("Provider")` over the predefined `cloud_then_ollama()` set, `.initial_value(current_default)`, `.filter_mode()`.
3. **Credential / endpoint resolution — BEFORE the fetch (review pt 3).** Determine the effective key for the selected cloud provider with precedence **env var > inline config key > `cliclack::password` prompt**; if the user skips the prompt (empty), proceed key-less. For Ollama, resolve/prompt the **endpoint** before `/api/tags` (reuse `effective_ollama_endpoint` + `validate_endpoint_url`). The key is held in memory only, never echoed/printed, and is persisted (inline, `0600`) **only** if the wizard completes (D8) — no partial writes.
4. `spinner()` "Fetching supported models…" around `fetch_supported_models(id, key, endpoint)`; `s.stop(...)` with the outcome (live count / fallback warning). Key present → live fetch; key still absent → skip the call, fallback + notice (D7.2).
5. `multiselect("Enable models")` over the post-processed list (D7), `.filter_mode()` (type-to-narrow), `.initial_values(current_enabled)`; require ≥1 selected (re-prompt/validate).
6. `select("Default model")` among the just-selected models, `.initial_value(current_default_model)`.
7. Assemble via **pure** `build_provider_config(...)` → `merge_provider_config(load(), updated, make_default=true)` (D8); persist with `config::save` (atomic `0600`); `outro(...)` confirmation.

Re-running pre-selects current enabled models and highlights the current default (AC-5). A fetch failure inside the spinner degrades to fallback and never crashes (AC-2). **Cancellation (Esc/Ctrl-C, review pt 16):** abort with `outro_cancel`/stderr message and a non-zero exit; **no partial config write**, no key printed, existing config left untouched.

### D6 — `http::get_json`

`post_json` only does POST. Add a sibling `get_json` that GETs and reuses the same `auth`/`extra_headers`/timeout/classify machinery. Note `HttpRequest` currently requires `payload: &Value` (POST-only); a GET has no body, so `get_json` either takes a slimmed request (endpoint + `auth` + `extra_headers`, no payload) or a `HttpRequest` whose payload is ignored — prefer a small dedicated GET request struct to keep the POST type honest. For the wizard, fetches use a short timeout and minimal/zero retries (UX: don't stall the spinner on a flaky network — fall back fast). The static fallback lists are discovery **catalogs** (like `default_model()`), not the resolved model, so they don't violate the ADR "no hardcoded model IDs in the resolved path" compliance check.

### D7 — Model-list hygiene (fetch post-processing)

The raw fetch result is **not** presented as-is. `fetch_supported_models` post-processes it (all pure, unit-testable on a `&str` body):

- **D7.1 — Chat-only filter (review pt 2).** `/v1/models`-style endpoints return *every* model, including non-text ones — OpenAI (`whisper-1`, `tts-1`, `dall-e-3`, `text-embedding-3-*`, `omni-moderation-*`), Groq (`whisper-large-v3`, `distil-whisper-*`). A user picking `dall-e-3` as default would fail at commit time. Each provider gets a `keep_chat_model(id) -> bool`: OpenAI/Groq use an **exclude-list** (substring match on `whisper`/`tts`/`dall-e`/`embedding`/`moderation`/`guard`/`babbage`/`davinci`) — safer than an include-list because new chat models aren't missed; Gemini filters on the authoritative `supportedGenerationMethods ∋ "generateContent"`; Anthropic (`claude-*` only) and Ollama (locally pulled) pass through.
- **D7.2 — No-key short-circuit (review pt 3).** A live fetch needs a key for OpenAI/Anthropic/Groq/Gemini. The wizard resolves the key first (D5 step 3: env > config > prompt). If the user *also* skips the prompt so the key is still `None`/blank, **skip the network call**, use the fallback list, and print an explicit cliclack note ("No `{KEY_ENV}` found — showing the built-in model list; provide the key for the live catalog."). Avoids a confusing silent 401→fallback.
- **D7.3 — Baseline merge (review pt 4).** Even on a *successful* fetch, the live list can omit a known-good model (new alias not yet listed, or the user's current default). Always **merge** `fetched ∪ static_fallback(id) ∪ {current default, current enabled set}` and **dedupe** (stable order: live first, then any missing baselines), so `.initial_value()`/`.initial_values()` always resolve and the current default is always selectable.
- **D7.4 — Pagination (review pt 6).** Gemini (`nextPageToken`) and Anthropic (`has_more`/`last_id`) paginate; OpenAI/Groq do not. v1 does **not** implement page-following — instead it requests a large first page (`pageSize=1000` / `limit=1000`), which covers realistic catalogs. Known limitation: an org with >1000 fine-tuned models would see a truncated list (mitigated by D7.3 keeping baselines selectable). Noted for a follow-up if it ever bites.

Order: fetch (or skip per D7.2) → filter (D7.1) → merge baselines (D7.3) → dedupe → sort. The filtered+merged list is what the multiselect shows.

`fetch_supported_models` returns a small outcome struct, **not** a bare `Vec` (review pt 10), so the wizard can show the right spinner-stop text and warning:

```rust
pub struct ModelFetchOutcome { pub models: Vec<String>, pub source: ModelSource2, pub warning: Option<String> }
pub enum ModelSource2 { Live, Fallback }   // Live "Fetched N models." | Fallback "Could not fetch (…); using built-in list."
```

### D8 — Provider-config merge semantics (the second data-loss risk)

`Config` holds `Vec<ProviderConfig>` + a `default`; `save()` **overwrites the whole file** (`write_atomic`). The wizard selects **one** provider, so it must NOT write a `Config` containing only that provider — that would silently delete every other provider's key/endpoint/model/whitelist (review pt 1). Required behavior, via a pure helper:

```rust
/// Update exactly one provider in an existing config (add if absent), preserving
/// all others verbatim; optionally make it the new default. Pure, unit-testable.
fn merge_provider_config(existing: Option<&Config>, updated: ProviderConfig, make_default: bool) -> Config
```

- Loads the current config (if any) with `config::load()`.
- Replaces only the matching `ProviderConfig` (by `id`); appends if absent; leaves the rest untouched.
- Sets `Config.default = updated.id` (the just-configured provider becomes the default for a bare `gcm` — the wizard's whole point; documented, not silent).
- Writes `version = 2` (D3).

**Cross-wizard preservation (review pt 2).** D1 keeps `gcm config`/onboarding's `run_wizard` unchanged — but once `ProviderConfig.models` exists, `run_wizard` rebuilds each provider via `cloud_provider_config` with **no** `models`, so a subsequent `gcm config` / `gcm --reconfigure` would **erase a whitelist** set by `gcm provider`. Fix: `run_wizard`/`build_config` must **carry forward the existing `models` (and inline `model` default)** for each re-enabled provider by loading the current config first and merging per provider. This is a targeted `config.rs` change (same module) and is required to avoid silent whitelist loss. (Alternative — route `gcm config` to the new wizard — is deferred as out of scope, D1.)

---

## Implementation plan

### Phase 1 — Schema + enforcement (pure, no TUI, no network)
- [ ] Add `ProviderConfig.models: Vec<String>` (`#[serde(default, skip_serializing_if = "Vec::is_empty")]`).
- [ ] Bump `CONFIG_FORMAT_VERSION` to 2; migrate v1 in `parse_config` (accept 1 & 2; reject 0 / >2).
- [ ] `model_is_enabled(cfg, id, model)` pure helper (empty = unrestricted; non-empty = membership) + per-provider `canonicalize_model` (Gemini strip `models/`, Ollama `:latest`, trim) used on both sides (pt 17).
- [ ] Wire enforcement into `main.rs::ensure_configured()` after `apply_to_env` (while `cfg` is live — see D4), using `pick_provider_id` + `resolve_model_with_source`; clear `GcmError::Config` message.
- [ ] `merge_provider_config(existing, updated, make_default)` (D8, pt 1); update `run_wizard`/`build_config` to **preserve existing `models`** per re-enabled provider (pt 2).
- [ ] `render_config` forces serialized `version = CONFIG_FORMAT_VERSION` (pt 9); update `commented_reference`/`sample_toml_template`/`non_tty_instructions` (`version = 2`, `models = [...]`).
- [ ] Unit tests: migration (v1 loads + stamps v2, v2 round-trips, v0/v3 rejected), enforcement matrix incl. canonicalization, `merge_provider_config` preserves others + sets default, `run_wizard` preserves `models`, render round-trip.

### Phase 2 — Model fetching + hygiene (sync, fallback, injectable)
- [ ] `http::get_json` (GET request; large first page via `pageSize=1000`/`limit=1000` where the provider paginates — D7.4).
- [ ] `src/provider/models.rs`: `fetch_supported_models` + `static_fallback_models(id)`; per-backend endpoint/parse helpers (`pub(crate)` in each backend module).
- [ ] `keep_chat_model(id)` per provider (exclude-list for OpenAI/Groq; `generateContent` for Gemini; passthrough Anthropic/Ollama) — D7.1.
- [ ] No-key short-circuit + notice (D7.2); baseline merge + dedupe (D7.3).
- [ ] Pure unit tests: parse each provider's sample JSON → ids; chat-only filter drops `whisper`/`tts`/`dall-e`/`embedding` (and keeps `gpt-*`/`claude-*`/`gemini` generateContent); failure/empty/no-key → fallback; merged list always contains `default_model()` + current default. (Parse/filter/merge take a `&str` body / `Vec` so they're network-free.)

### Phase 3 — CLI + cliclack wizard
- [ ] `Cargo.toml`: `cliclack`/`console` with Goose-style `default-features = false` flags (pt 15); record `cargo tree` delta.
- [ ] `Commands::Provider` + `run_provider_subcommand()` dispatch (early, non-TTY guard → guidance + exit 1).
- [ ] `src/provider/wizard.rs`: cliclack flow (credential resolution env>config>`password` *before* fetch, pt 3; spinner uses `ModelFetchOutcome`; `merge_provider_config` persist; cancel → `outro_cancel` + non-zero, no partial write, pt 16) + pure `build_provider_config`.
- [ ] Pure unit tests for the assembly/validation helpers.

### Phase 4 — Integration tests, docs, polish
- [ ] `tests/provider.rs`: non-TTY guard; v1-config-loads-after-bump; non-empty-`models` rejects an out-of-set `--model`; empty-`models` allows free-form; enforcement on a clean/no-op repo errors (intentional, pt 13); `gcm config` after `gcm provider` preserves the `models` whitelist (pt 2).
- [ ] README + reference-block docs.
- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, full `cargo test` green.
- [ ] Manual TTY verification of the cliclack flow against a large list (Ollama/OpenAI) — record in the PR.

### Deferred (middle severity — owner said skip; tracked here, address during impl if cheap)
- **pt 4** `.max_rows(~20)` on the model multiselect (and selects) — strongly recommended for large lists; trivially added at impl time.
- **pt 11** concrete fast-fetch transport — `get_json` variant with a 3-5s timeout + 0-1 retries (vs the 60s/3-retry commit defaults) so the spinner can't feel hung.
- **pt 12** `gcm status` enabled-models awareness (count / is-allowed) — out of CLO-516 scope; follow-up to avoid a status-vs-runtime split-brain.
- **pt 14** module visibility — wizard in `provider/wizard.rs` can't reach `config.rs`-private helpers (`cloud_then_ollama`, `provider_label`, `provider_token`, `validate_endpoint_url`, …). Resolve at impl: lift the shared provider label/order/token helpers to `provider/mod.rs` (single source of truth) or mark the needed `config.rs` helpers `pub(crate)` — do **not** duplicate the provider label/order tables.

---

## Constraints

**Must**:
- Keep the `Provider` trait **synchronous**; no `tokio`/async anywhere (ADR-001 Decision 2; CI greps for `async fn`/tokio).
- Preserve config precedence `flag > env > config > default` (CLO-496 invariant) and atomic `0600` writes (FR-55).
- Existing v1 configs MUST load without error after the version bump (migration), and migration MUST stamp `version = CONFIG_FORMAT_VERSION` so a re-save persists v2 (pt 9).
- `gcm provider` MUST preserve all *other* providers' settings (D8, pt 1), and `gcm config`/`run_wizard` MUST preserve an existing per-provider `models` whitelist when rebuilding (pt 2).
- Secrets: keys stay env-var-referenced or inline-only-at-`0600`; never echo a key to stdout/JSON; the model list and selections carry no secrets; Gemini key goes in the `x-goog-api-key` header, never the URL query.

**Must-not**:
- Must not crash the wizard on a model-fetch failure — always degrade to the static fallback.
- Must not break the existing `gcm config` / onboarding flow or its tests.
- Must not hang on a non-TTY: `gcm provider` without a terminal fails fast with guidance + non-zero exit (mirror `gcm config`).
- Must not silently start rejecting a v1 user's free-form models (empty `models` = unrestricted).
- Must not write a partial config or print a key on cancel/error mid-wizard — persist only on completion, leave the existing config untouched (pt 16).
- Must not falsely reject a model that differs only by canonicalization (Gemini `models/`, Ollama `:latest`) — compare after canonicalizing both sides (pt 17).

**Prefer**:
- Reuse existing infrastructure (`http`, `ProviderId` env/default helpers, `config` atomic write, `cloud_then_ollama`) over new code.
- Pure functions for all testable logic; keep cliclack confined to a thin IO shell.
- `.filter_mode()` (zero deps) before any fuzzy-matcher crate.

**Escalate when**:
- The model-fetch or whitelist work would require breaking the `Provider` trait signature or going async → stop and confirm.
- `cliclack`/`console` drag in a heavyweight or duplicate dep subtree that conflicts with the lean-binary goal (FR-41) → surface the `cargo tree` delta before committing.
- Migrating/redirecting `gcm config` or first-run onboarding to cliclack proves necessary → that's a separate decision (Decision 11 boundary).

---

## Acceptance criteria

- [ ] **AC-1 (UI):** `gcm provider` in a TTY shows the Goose-style interface — header chip, ◇/◆ radio list, green highlight, spinner — via `cliclack`. *Verify:* manual TTY run (PR screenshot/recording).
- [ ] **AC-2 (live fetch + fallback):** Selecting a provider triggers a live model fetch (spinner); on failure it falls back to a static list without crashing. *Verify:* unit test — fetch parse error/empty/transport → returns fallback; merged list ⊇ `{default_model()}` (D7.3).
- [ ] **AC-3 (filter multiselect):** The model multiselect narrows by typed letters; `space` toggles, `enter` submits. *Verify:* manual TTY run against a large list (Ollama/OpenAI).
- [ ] **AC-4 (multiselect + one default):** User can enable 1..N models and pick exactly one default among them. *Verify:* unit test on `build_provider_config` — default ∈ enabled; ≥1 enabled required.
- [ ] **AC-5 (persist + pre-select):** Selections persist to `config.toml` (`version = 2`, `models = [...]`, `model = <default>`); re-running pre-selects current enabled + highlights current default. *Verify:* unit round-trip test + manual re-run.
- [ ] **AC-6 (enforcement):** A `--model`/env/config value outside a non-empty enabled set is rejected with a clear message. *Verify:* `tests/provider.rs` integration — config with `models=["a"]`, `--model b` → error code `Config`, message names `b` + lists enabled; **and** empty `models` allows any model.
- [ ] **AC-7 (back-compat):** Existing v1 configs load without error after the bump. *Verify:* unit test (`parse_config` on a `version = 1` body) + `tests/provider.rs` (a v1 `config.toml` hydrates, not `OnboardingRequired`).
- [ ] **AC-8 (chat-only filter, D7.1):** Non-text models are excluded from the picker. *Verify:* unit — OpenAI/Groq sample bodies containing `whisper-*`/`tts-*`/`dall-e-*`/`*-embedding-*` yield a list with none of them but keeping `gpt-*`/`openai/gpt-oss-*`; Gemini keeps only `generateContent` models.
- [ ] **AC-9 (no-key short-circuit, D7.2):** With no key set, `gcm provider`'s fetch is skipped and the fallback list + an explicit notice are shown (no silent 401). *Verify:* unit — `fetch_supported_models(id, None, …)` returns fallback without a network call; manual TTY for the notice.

**Verification method:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` all green; AC-1/AC-3 by manual TTY run recorded in the PR.

---

## Evaluation

| # | Test | Expected | Command / Steps |
|---|------|----------|-----------------|
| 1 | v1 config parses after bump | `Ok`, `models == []` | unit: `parse_config("version = 1 …")` |
| 2 | v2 config round-trips with `models` | `models` preserved, `version == 2` | unit: render → parse |
| 3 | version 0 / 3 rejected | `WrongVersion` | unit: `parse_config` |
| 4 | empty `models` = unrestricted | `Ok(())` for any model | unit: `model_is_enabled(cfg, id, "anything")` |
| 5 | non-empty `models` rejects out-of-set | `Err` naming model + set | unit + `tests/provider.rs` (`--model` not enabled) |
| 6 | fetch failure → fallback | returns `static_fallback_models(id)` ⊇ default | unit: parse error / closed port |
| 7 | each provider's sample JSON parses to ids | exact id list | unit per backend (network-free, `&str` body) |
| 8 | `build_provider_config` requires default ∈ enabled | `Err` if default not selected | unit |
| 9 | `gcm provider` non-TTY | exit ≠ 0 + guidance on stderr | `tests/provider.rs` (`stdin` null) |
| 10 | default model always selectable offline | `default_model()` ∈ merged list | unit per provider |
| 11 | chat-only filter (D7.1) | `whisper`/`tts`/`dall-e`/`embedding` dropped; `gpt-*`/`claude-*` kept | unit per provider (OpenAI/Groq exclude-list; Gemini `generateContent`) |
| 12 | no-key short-circuit (D7.2) | no network call; returns fallback | unit: `fetch_supported_models(id, None, …)` |
| 13 | baseline merge on success (D7.3) | live list missing the current default → default still present after merge | unit: merge(live, fallback, current) ⊇ {default, current} |
| 14 | `merge_provider_config` preserves others (D8, pt 1) | 2-provider config + update one → other untouched, default = updated | unit |
| 15 | `gcm config` preserves whitelist (pt 2) | re-run after `gcm provider` keeps `models = [...]` | unit on `run_wizard`/`build_config` + `tests/provider.rs` |
| 16 | migration stamps + re-saves v2 (pt 9) | load v1 → save → file has `version = 2` | unit: `parse_config` v1 → `render_config` contains `version = 2` |
| 17 | canonicalized match (pt 17) | `--model llama3` allowed when `llama3:latest` enabled; Gemini `models/x` vs `x` | unit: `model_is_enabled` after `canonicalize_model` |

**Edge cases:** provider API returns 200 but empty `data` → fallback; huge model list (Ollama/OpenAI) → `.filter_mode()` stays responsive; Gemini `models/`-prefixed names stripped + non-`generateContent` filtered; Ollama unreachable → `/api/tags` fails → fallback `[default]`; OpenAI/Groq `whisper`/`tts`/`embedding` filtered out (pt 2); no key set → fetch skipped + notice (pt 3); live fetch omits current default → merged back in (pt 4); >1000 models → first page only, baselines still selectable (pt 6); invalid `GCM_PROVIDER` unaffected (wizard selects explicitly); migrating a v1 config that set a free-form `--model` keeps working (empty `models`).

---

## Testing strategy

- **Unit tests** (the bulk): config migration + version gating; `model_is_enabled` enforcement matrix; per-provider model-list **parse** functions (take a `&str` body — network-free); fallback behavior; `build_provider_config` (default ∈ enabled, ≥1 enabled); render/round-trip. Mirrors how CLO-496 unit-tested `build_config`/`parse_selection`/`validate_endpoint_url`.
- **Integration tests** (`tests/provider.rs`, subprocess + `GCM_CONFIG` + `stdin(Stdio::null())`, mirroring `tests/onboarding.rs`): non-TTY `gcm provider` fails fast with guidance; a saved v1 config hydrates (not `OnboardingRequired`); a `models=["x"]` config + `--model y` is rejected with the `Config` code; an empty-`models` config allows free-form `--model`. No network (point base URLs at a closed port; cap `GCM_HTTP_TIMEOUT_SECS`/`GCM_RETRY_MAX`).
- **Manual TTY** (AC-1, AC-3): the cliclack flow can't be driven by piped stdin (it reads `/dev/tty`), and there is no PTY harness / dev-dependency in this repo. So the interactive UI is verified by hand against a large model list and recorded in the PR — exactly as CLO-496's interactive wizard itself is manually verified while its pure logic is unit-tested.

---

## Open questions

- [ ] **Q1:** Should a later task delegate `gcm config` (and/or first-run onboarding) to the cliclack wizard for one interactive path? (Out of scope here; Decision 11 boundary.)
- [ ] **Q2:** Fetch retry/timeout budget for the spinner — confirm "fast fallback" (short timeout, ≤1 retry) is the desired UX vs. waiting longer for a live list.
- [ ] **Q3:** Do we cache the fetched model list (per provider, short TTL) to make re-runs instant, or always re-fetch? (Lean toward always-fetch in v1; note for later.)

---

## References

- [Linear CLO-516](https://linear.app/cloud-ai/issue/CLO-516)
- [ADR-001](../adrs/001-foundational-architecture-decisions.md) — Decision 2 (sync client), Decision 4 (TOML/versioned config), Decision 11 (minimal onboarding, no forced model selection)
- [CLO-496 onboarding design](clo-496-onboarding-wizard.md) — config schema + wizard precedent
- Goose: `github.com/block/goose` — `crates/goose-cli/src/commands/configure.rs`, `crates/goose-providers/src/base.rs`
- Code touchpoints: `src/config.rs`, `src/provider/mod.rs`, `src/provider/http.rs`, `src/provider/groq.rs`, `src/cli.rs`, `src/main.rs`
