# Design: CLO-596 — Expose provider identity and the live model registry through the gcm library

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-596
**Status**: Design
**Created**: 2026-07-28
**Discovery**: [docs/discovery/clo-596.md](../discovery/clo-596.md)
**PRD**: [docs/prds/clo-596-expose-provider-identity-and-registry.md](../prds/clo-596-expose-provider-identity-and-registry.md)
**ADR**: [docs/adrs/002-library-boundary.md](../adrs/002-library-boundary.md)
**Approach**: Approach B — file-level split with a binary facade

---

## Problem

`src/provider/mod.rs` (~1,000 lines) interleaves two audiences: library-ready identity and registry types (`ProviderError`, `ErrorKind`, `ProviderId`, `AuthMethod`, `ModelSource`, `resolve_model_with_source`) and binary-only commit-domain types (`Provider` trait, `ConflictHunk`, `ResolveContext`, `Resolution`, `select()`, per-provider backends). `ProviderId` unconditionally derives `clap::ValueEnum`, blocking the no-`clap` library build that ADR-002 Decision 4 mandates. `src/provider/models.rs` and `src/provider/http.rs` are already free of `crate::plan`/`crate::diff` references and could cross the boundary today, but they cannot ship ahead of the identity types because `models.rs` names `super::ProviderId` 94 times and takes `super::ProviderError` in its fetcher signature. Discovery scored the baseline 6/10: well-tested code with a clean injectable-fetcher seam, but the extraction requires splitting a large interleaved file and resolving two dependency traps (`clap` on `ProviderId`, `debug_log!` in `http.rs`).

## Goals / Non-goals

**Goals:**

- Split `src/provider/mod.rs` into a library module (`identity.rs` + re-exports) and a binary facade (`facade.rs`), following the CLO-595 `#[path]` pattern.
- Export from the library: `ProviderId`, `AuthMethod`, `ModelSource`, `ProviderError`, `ErrorKind`, `resolve_model_with_source`, and the model registry (`models::{fetch_supported_models_with, fetch_supported_models, ModelFetchOutcome, FetchSource}`).
- Move `http.rs` and `models.rs` behind the library boundary so the registry has a single home.
- Make `ProviderId`'s `clap::ValueEnum` derive conditional on `feature = "clap"` per ADR-002 Decision 4, and provide a clap-free `parse` method.
- Keep `Provider` trait, `select()`, `ConflictHunk`, `ResolveContext`, `Resolution`, all six per-provider backends, shared prompts, and vertex ADC helpers in the binary.
- Maintain type identity: the binary's `crate::provider::ProviderId` is `gcm::provider::ProviderId` (re-exported from the library, not re-declared).
- Add library unit tests that exercise `resolve_model_with_source` with an injected env map and `fetch_supported_models_with` with an injected fetcher returning the static fallback, touching no network.
- `cargo test --no-default-features` compiles the library provider surface.

**Non-goals:**

- No extraction of `status.rs` (CLO-597) or `config.rs` (ADR-002 Decision 3, future ticket).
- No extraction of the `Provider` trait or any backend implementation.
- No change to `--provider` CLI semantics, `gcm provider` wizard behavior, or exit codes.
- No crates.io publish. Path dependency only (ADR-002 Decision 7).
- No async migration, no transport refactor (ADR-002 Decision 2).

## Architecture

### Crate shape (unchanged from CLO-595)

```
gcm package (one Cargo.toml, two targets)

  [lib] gcm  -> src/lib.rs
    pub mod privacy;                 (CLO-595)
    pub mod provider;                src/provider/mod.rs    ← library content
      pub mod identity;              src/provider/identity.rs  NEW
      #[doc(hidden)] pub mod debug;  src/debug.rs           ← compiled in lib too
      pub mod http;                  src/provider/http.rs   ← moved behind boundary
      pub mod models;                src/provider/models.rs ← moved behind boundary

  [[bin]] gcm (required-features = ["cli"]) -> src/main.rs
    #[path = "privacy/facade.rs"] mod privacy;   (CLO-595)
    #[path = "provider/facade.rs"] mod provider;  src/provider/facade.rs  NEW
      Provider trait, select(), ConflictHunk, ResolveContext, Resolution
      mod groq, gemini, openai, anthropic, ollama, vertex
      pub use gcm::provider::{ProviderId, ProviderError, ErrorKind, ...}
    mod debug;                       src/debug.rs  ← binary keeps its own copy
    mod cache, cli, config, diff, error, git, output, paths, plan, resolve, status, ui
```

The binary's `mod provider;` declaration changes to `#[path = "provider/facade.rs"] mod provider;`, exactly as CLO-595 did for `privacy`. The library's `pub mod provider;` resolves to `src/provider/mod.rs`, which now contains only library code. `src/debug.rs` is compiled in both crates (each declares `mod debug;`), so `crate::debug_log!` resolves correctly in both — no callsite changes needed.

### File-by-file changes

| Path | Change |
|---|---|
| `src/lib.rs` | Add `pub mod provider;` and `#[doc(hidden)] pub mod debug;` |
| `src/provider/mod.rs` | **Rewritten** — library module: `pub mod identity; pub mod http; pub mod models;` + re-exports from `identity` |
| `src/provider/identity.rs` | **New** — `ProviderError`, `ErrorKind`, `ProviderId`, `AuthMethod`, `ModelSource`, `resolve_model_with_source`, `env_u64`, `is_retryable`, `retry_after_hint` moved verbatim from `mod.rs` |
| `src/provider/facade.rs` | **New** — `Provider` trait, `select()`, `pick_provider_id`, `resolve_model`, `ConflictHunk`, `ResolveContext`, `Resolution`, shared prompts, resolve helpers, vertex ADC probes, `mod groq/gemini/openai/anthropic/ollama/vertex`; `pub use gcm::provider::{...}` re-exports |
| `src/provider/http.rs` | Items widened from `pub(super)` → `pub`; `ureq`-using functions gated `#[cfg(feature = "cli")]`; `crate::debug_log!` works (library has `mod debug;`) |
| `src/provider/models.rs` | `fetch_supported_models_with` made `pub`; `fetch_supported_models` gated `#[cfg(feature = "cli")]`; imports from `super::identity` instead of `super` |
| `src/provider/groq.rs` etc. | `use super::http` → `use gcm::provider::http`; `use super::{ProviderId, ProviderError, ...}` → resolved via facade re-exports |
| `src/main.rs` | `mod provider;` → `#[path = "provider/facade.rs"] mod provider;` |
| `src/cli.rs` | No change — `use crate::provider::ProviderId` resolves to facade re-export |
| `src/config.rs` | No change — `use crate::provider::{AuthMethod, ProviderId}` resolves to facade re-exports |
| `src/status.rs` | No change — `use crate::provider::{ollama, resolve_model_with_source, ...}`; `ollama` stays `pub(crate)` in facade |
| `src/error.rs` | No change — `use crate::provider::ProviderError` resolves to facade re-export |
| `src/output.rs` | No change — `use crate::provider::{ErrorKind, ProviderError}` resolves to facade re-exports |
| `Cargo.toml` | No change — feature setup already correct from CLO-594/CLO-595 |
| `tests/library_provider_api.rs` | **New** — library consumer test |

### Data flow

```
  external crate (lok / future picker)       gcm binary
  -----------------------------------       ----------
  gcm::provider::ProviderId::Groq           crate::provider::ProviderId::Groq
          |                                         |
          |                                facade re-exports identity
          |                                         |
  gcm::provider::resolve_model_with_source  crate::provider::select()
    (id, cli, env_lookup) -> (model, src)     (cli_provider, cli_model)
          |                                         |
          |                                resolve_model(id, cli)
          |                                  calls gcm::provider::resolve_model_with_source
          |                                         |
          v                                         v
  gcm::provider::models::                    provider backends (groq, gemini, ...)
  fetch_supported_models_with                 http::post_json (ureq, cli-gated)
    (id, key, ep, proj, fetch) -> outcome
```

### ProviderId clap dependency resolution

Current (line ~333 of `mod.rs`):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[value(rename_all = "lower")]
#[serde(rename_all = "lowercase")]
pub enum ProviderId { ... }
```

After (in `identity.rs`):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "clap", value(rename_all = "lower"))]
#[serde(rename_all = "lowercase")]
pub enum ProviderId { ... }
```

Per lesson L2, the `cli` feature already includes `"clap"` (not just `"dep:clap"`) in its dependency list, so `cfg_attr(feature = "clap", ...)` fires correctly under default features.

### ProviderId::parse without clap

Current:
```rust
pub(crate) fn parse(s: &str) -> Option<Self> {
    <ProviderId as ValueEnum>::from_str(s.trim(), true).ok()
}
```

After (in `identity.rs`) — a hand-rolled parser that honors aliases:
```rust
pub fn parse(s: &str) -> Option<Self> {
    match s.trim().to_lowercase().as_str() {
        "groq" => Some(Self::Groq),
        "google" | "gemini" => Some(Self::Google),
        "openai" => Some(Self::Openai),
        "anthropic" => Some(Self::Anthropic),
        "ollama" => Some(Self::Ollama),
        "vertex" | "google-vertex" => Some(Self::Vertex),
        _ => None,
    }
}
```

The CLI still derives `ValueEnum` (which provides its own `from_str` with aliases), but `ProviderId::parse` is now a library-safe method that does not depend on `clap`. The CLI's `--provider` flag uses clap's derive, not `ProviderId::parse` directly. The `pick_provider_id` function (in the facade) calls `ProviderId::parse` for env-var parsing.

### debug_log! resolution

`src/debug.rs` is self-contained (no `use crate::` imports). Adding `#[doc(hidden)] pub mod debug;` to `src/lib.rs` compiles it in the library crate, where `#[macro_export]` makes `debug_log!` available as `crate::debug_log!` within the library. The binary keeps its own `mod debug;` in `main.rs`, so existing `crate::debug_log!` callsites in binary code are unchanged. The two copies are independent — `Level` is only used internally by the debug module, never passed across the crate boundary.

### http.rs visibility and cfg gating

Items widened from `pub(super)` to `pub` so the binary (a separate crate) can access them via `gcm::provider::http`:

| Item | Current visibility | New visibility | `cfg` gate |
|---|---|---|---|
| `HttpRequest` struct | `pub(super)` | `pub` | always |
| `HttpGet` struct | `pub(super)` | `pub` | always |
| `post_json` fn | `pub(super)` | `pub` | `#[cfg(feature = "cli")]` |
| `get_json` fn | `pub(super)` | `pub` | `#[cfg(feature = "cli")]` |
| `bad_request_detail` fn | `pub(super)` | `pub` | always |
| `truncate` fn | `pub(super)` | `pub` | always |
| `classify_status` fn | private | private | always |
| `RetryConfig` struct | private | private | `#[cfg(feature = "cli")]` |
| `retry_with` fn | private | private | `#[cfg(feature = "cli")]` |
| `send_once` / `get_once` | private | private | `#[cfg(feature = "cli")]` |
| `map_ureq_error` fn | private | private | `#[cfg(feature = "cli")]` |
| `timeout_secs` fn | private | private | `#[cfg(feature = "cli")]` |

The library's `mod.rs` declares `#[doc(hidden)] pub mod http;` to signal these are internal even though they're `pub`.

### models.rs changes

- `fetch_supported_models_with` (currently private `fn`) → `pub fn` (the library surface).
- `fetch_supported_models` (currently `pub fn`) → `pub fn` with `#[cfg(feature = "cli")]` (calls `http::get_json` which needs `ureq`).
- `static_fallback_models`, `keep_chat_model`, `fetch_live` → stay private (called by `fetch_supported_models_with`).
- Imports change from `super::http::{self, HttpGet}` and `super::ProviderId` to `super::http::{self, HttpGet}` and `super::identity::ProviderId` (or `super::ProviderId` if `mod.rs` re-exports).

### Type identity

Per lesson L1, the facade re-exports identity types from the library, not re-declares them:
```rust
// src/provider/facade.rs
pub use gcm::provider::{
    ProviderId, AuthMethod, ModelSource, ProviderError, ErrorKind,
    resolve_model_with_source,
};
pub use gcm::provider::models::{fetch_supported_models, FetchSource};
```

All existing `use crate::provider::ProviderId` callsites in `cli.rs`, `config.rs`, `status.rs`, `error.rs`, and `output.rs` resolve to the facade's re-export, which IS `gcm::provider::ProviderId`. No callsite changes needed outside the provider directory.

---

## Public API surface

### `src/lib.rs` additions

```rust
//! gcm as a library: types and pure functions shared with in-org consumers.
//!
//! The CLI lives in the `gcm` binary target; this crate carries no `clap`,
//! `cliclack`, or HTTP transport unless the `cli` feature is enabled.
//! See `docs/adrs/002-library-boundary.md`.

pub mod privacy;    // CLO-595
pub mod provider;   // CLO-596

#[doc(hidden)]
pub mod debug;      // compiled in lib so http.rs's debug_log! resolves
```

### `gcm::provider` (library)

```rust
// src/provider/mod.rs (library module)
pub mod identity;
pub mod http;
pub mod models;

pub use identity::{
    AuthMethod, ErrorKind, ModelSource, ProviderError, ProviderId,
    resolve_model_with_source,
};
```

### `gcm::provider::identity`

```rust
/// Typed, provider-agnostic failure taxonomy (FR-21).
#[derive(Debug)]
pub struct ProviderError {
    pub provider: &'static str,
    pub kind: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    MissingKey { env_var: &'static str },
    RateLimit { retry_after: Option<Duration> },
    Auth { status: u16, env_var: &'static str },
    BadRequest { detail: Option<String> },
    Server(u16),
    Http(u16),
    Timeout,
    Transport(String),
    EmptyResponse,
    Deserialize(String),
    Config(String),
}

impl ProviderError {
    pub fn new(provider: &'static str, kind: ErrorKind) -> Self;
}
impl std::fmt::Display for ProviderError {}
impl std::error::Error for ProviderError {}

/// The selectable providers (FR-12). `--provider` accepts the lower-case names;
/// `google` also accepts the alias `gemini`; `vertex` accepts `google-vertex`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "clap", value(rename_all = "lower"))]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Groq,
    #[cfg_attr(feature = "clap", value(alias = "gemini"))]
    #[serde(alias = "gemini")]
    Google,
    Openai,
    Anthropic,
    Ollama,
    #[cfg_attr(feature = "clap", value(alias = "google-vertex"))]
    #[serde(alias = "google-vertex")]
    Vertex,
}

impl ProviderId {
    /// The provider's API key env var, or `None` for key-free Ollama/Vertex.
    pub fn key_env_var(self) -> Option<&'static str>;

    /// Default model id (ADR-001 Decisions 5/7).
    pub fn default_model(self) -> &'static str;

    /// Per-provider model env vars, in precedence order (primary first).
    pub fn model_env_vars(self) -> &'static [&'static str];

    /// Parse a provider name (env), case- and whitespace-insensitive, honoring
    /// the `gemini` and `google-vertex` aliases. Does not depend on clap.
    pub fn parse(s: &str) -> Option<Self>;

    /// Canonical lowercase token (e.g. `groq`, `google`, `vertex`).
    pub fn as_str(self) -> &'static str;

    /// How this provider authenticates.
    pub fn auth_method(self) -> AuthMethod;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    ApiKey,
    KeylessEndpoint,
    KeylessAdc,
}

/// Where a resolved model value came from (CLO-515 source attribution).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    Flag,
    Env(&'static str),
    Default,
}

/// Resolve the effective model and its source for a provider (CLO-515).
/// Precedence: flag > per-provider env in order > default. Empty/whitespace
/// flag and env values are skipped. `env_lookup` is injected so callers
/// stay hermetic.
pub fn resolve_model_with_source(
    id: ProviderId,
    cli: Option<&str>,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> (String, ModelSource);
```

### `gcm::provider::models`

```rust
/// Where a model list came from (live endpoint or static fallback).
pub enum FetchSource {
    Live,
    Fallback,
}

/// The outcome of a model-list fetch: models, source, optional warning.
pub struct ModelFetchOutcome {
    pub models: Vec<String>,
    pub source: FetchSource,
    pub warning: Option<String>,
}

/// Fetch models using a caller-supplied fetcher. Never errors: returns a
/// usable list in every case. The fetcher receives an `HttpGet` and returns
/// the raw response body or a `ProviderError`. No network is touched when
/// the fetcher returns the static fallback.
pub fn fetch_supported_models_with(
    id: ProviderId,
    key: Option<&str>,
    endpoint: Option<&str>,
    project: Option<&str>,
    fetch: impl Fn(&HttpGet) -> Result<String, ProviderError>,
) -> ModelFetchOutcome;

/// Convenience wrapper: uses `http::get_json` as the fetcher.
/// Available only with the `cli` feature (requires `ureq`).
#[cfg(feature = "cli")]
pub fn fetch_supported_models(
    id: ProviderId,
    key: Option<&str>,
    endpoint: Option<&str>,
    project: Option<&str>,
) -> ModelFetchOutcome;
```

### `gcm::provider::http`

Internal (`#[doc(hidden)] pub mod http`), but items are `pub` for binary access:

```rust
pub struct HttpRequest<'a> {
    pub provider: &'static str,
    pub auth_env_var: &'static str,
    pub endpoint: String,
    pub auth: Option<(&'static str, String)>,
    pub extra_headers: Vec<(&'static str, String)>,
    pub payload: &'a Value,
}

pub struct HttpGet {
    pub provider: &'static str,
    pub auth_env_var: &'static str,
    pub endpoint: String,
    pub auth: Option<(&'static str, String)>,
    pub extra_headers: Vec<(&'static str, String)>,
}

#[cfg(feature = "cli")]
pub fn post_json(req: &HttpRequest) -> Result<String, ProviderError>;

#[cfg(feature = "cli")]
pub fn get_json(req: &HttpGet) -> Result<String, ProviderError>;

pub fn bad_request_detail(body: &str) -> Option<String>;
pub fn truncate(s: &str, max: usize) -> String;
```

---

## Assumptions

| # | Assumption | Confidence | Verification |
|---|---|---|---|
| A1 | `src/debug.rs` has no `use crate::` imports and can be compiled in the library crate without changes. | high | Verified by grep during discovery (`grep "use crate::" src/debug.rs` → no results). |
| A2 | The `cli` feature in `Cargo.toml` already includes `"clap"` (not just `"dep:clap"`), so `cfg_attr(feature = "clap", ...)` fires under default features. | high | Verified by reading `Cargo.toml` line 18: `cli = ["clap", "dep:cliclack", ...]`. Per lesson L2. |
| A3 | The binary's existing `use crate::provider::ProviderId` callsites resolve to the facade's `pub use gcm::provider::ProviderId` re-export without changes, maintaining type identity. | high | Structural: re-exports are transparent in Rust. Per lesson L1, the key is re-exporting, not re-declaring. |
| A4 | `ProviderId::parse` can be hand-rolled to match `clap::ValueEnum::from_str`'s alias behavior (`gemini` → Google, `google-vertex` → Vertex) without behavioral change. | high | The aliases are a fixed 2-item mapping; the CLI's `--provider` flag uses clap's derive, not `ProviderId::parse`. |
| A5 | `status.rs`'s use of `crate::provider::ollama` (for `is_cloud_model`, `normalize_host`, `DEFAULT_BASE_URL`) continues to work because the facade declares `pub(crate) mod ollama;`. | high | `status.rs` is binary-only in this ticket; the facade is the binary's `provider` module. |
| A6 | `http.rs`'s `crate::debug_log!` calls resolve correctly in the library because `src/debug.rs` is compiled as `pub mod debug;` in `lib.rs`. | high | `#[macro_export]` makes the macro available at the library crate root. |
| A7 | `ureq`-using functions in `http.rs` can be gated behind `#[cfg(feature = "cli")]` without breaking the library build, since `fetch_supported_models_with` (the pure surface) does not call them. | high | `fetch_supported_models_with` takes an injected fetcher; only the convenience wrapper `fetch_supported_models` calls `http::get_json`. |

---

## Test plan

### Unit tests (library, `src/provider/identity.rs`)

1. `resolve_model_with_source` returns `(model, Flag)` when CLI model is non-empty.
2. `resolve_model_with_source` returns `(model, Env(var))` when env lookup yields a value and CLI is absent.
3. `resolve_model_with_source` returns `(default, Default)` when both CLI and env are absent.
4. `resolve_model_with_source` skips empty/whitespace CLI and env values.
5. `ProviderId::parse` accepts `groq`, `google`, `gemini` (alias), `openai`, `anthropic`, `ollama`, `vertex`, `google-vertex` (alias), case-insensitively.
6. `ProviderId::parse` rejects unknown names with `None`.
7. `ProviderId::key_env_var` returns `None` for Ollama and Vertex, `Some` for the rest.
8. `ProviderId::auth_method` returns `KeylessEndpoint` for Ollama, `KeylessAdc` for Vertex, `ApiKey` for the rest.

### Unit tests (library, `src/provider/models.rs`)

9. `fetch_supported_models_with` with an injected fetcher returning the static fallback body yields `FetchSource::Fallback` with a non-empty model list and no network call.
10. `fetch_supported_models_with` with an injected fetcher returning `Err(ProviderError)` degrades to fallback with a warning.
11. `fetch_supported_models_with` with an injected fetcher returning a live body yields `FetchSource::Live` with filtered/deduped models.
12. `fetch_supported_models_with` with `key=None` for a key-bearing provider short-circuits to fallback with a key warning.
13. `fetch_supported_models_with` with `key=None` for Vertex short-circuits to fallback with an ADC warning.

### Integration tests (binary, existing — unchanged in intent)

14. `tests/provider.rs` — wizard model whitelist, v1→v2 config migration (unchanged).
15. `tests/vertex.rs` — Vertex AI mock server end-to-end (unchanged).

### New integration test (`tests/library_provider_api.rs`)

16. A test that imports `gcm::provider::{ProviderId, resolve_model_with_source, ModelSource}` and resolves a model using a caller-supplied env map (no `std::env::var`).
17. A test that imports `gcm::provider::models::fetch_supported_models_with` and drives it with an injected fetcher returning the static fallback, touching no network.

### Manual / HITL

18. `gcm --provider groq --dry-run` still resolves and prints the provider/model.
19. `gcm --provider gemini --dry-run` alias still accepted.
20. `gcm --provider google-vertex --dry-run` alias still accepted.
21. `gcm provider` wizard lists live Vertex and Google models (CLO-564 behavior intact).

### Build matrix

22. `cargo test` — 489 tests still pass (baseline).
23. `cargo test --no-default-features` — library compiles and provider unit tests pass.
24. `cargo build --no-default-features` — library compiles without `clap`/`ureq`/`cliclack`.

---

## Migration / rollout

1. **No user-facing change.** The binary's CLI flags, wizard, and commit flow are behaviorally identical. The extraction is purely structural.
2. **No `Cargo.toml` change.** The feature setup from CLO-594/CLO-595 is already correct: `default = ["cli"]`, `cli = ["clap", "dep:cliclack", ...]`, `clap = ["dep:clap"]`, `[[bin]] required-features = ["cli"]`.
3. **Import migration is internal to `src/provider/`.** The six per-provider files change `use super::http` to `use gcm::provider::http` and rely on facade re-exports for identity types. No changes to files outside `src/provider/` except `src/main.rs` (one line: `mod provider;` → `#[path = ...]`).
4. **`src/lib.rs` gains two lines** (`pub mod provider;` and `#[doc(hidden)] pub mod debug;`).
5. **Rollback** is a single revert commit — the extraction adds files and changes `#[path]` attributes, but does not delete or rename anything that a `git revert` cannot undo.

---

## Open questions

All three discovery-debt items are resolved by this design:

1. **`debug_log!` availability:** Resolved by compiling `src/debug.rs` in the library crate (`#[doc(hidden)] pub mod debug;` in `lib.rs`). The binary keeps its own `mod debug;`. No callsite changes.

2. **`fetch_supported_models` vs `ureq`:** Resolved by gating the convenience wrapper `fetch_supported_models` behind `#[cfg(feature = "cli")]` (it calls `http::get_json` which needs `ureq`). The injectable `fetch_supported_models_with` is the always-available library surface. The ADR lists `fetch_supported_models` in the surface; under default features (`cli` on), it IS available.

3. **`ProviderId::parse` without clap:** Resolved by a hand-rolled parser that matches on lowercased trimmed strings and honors the two aliases (`gemini` → Google, `google-vertex` → Vertex). The CLI's `--provider` flag continues to use clap's `ValueEnum` derive for its own parsing.