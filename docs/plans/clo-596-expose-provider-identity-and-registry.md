# Plan: CLO-596 — Expose provider identity and the live model registry through the gcm library

## Context

- **Design:** [docs/designs/clo-596-expose-provider-identity-and-registry.md](../designs/clo-596-expose-provider-identity-and-registry.md)
- **Discovery:** [docs/discovery/clo-596.md](../discovery/clo-596.md)
- **PRD:** [docs/prds/clo-596-expose-provider-identity-and-registry.md](../prds/clo-596-expose-provider-identity-and-registry.md)
- **ADR:** [docs/adrs/002-library-boundary.md](../adrs/002-library-boundary.md)
- **Linear:** https://linear.app/cloud-ai/issue/CLO-596
- **Approach:** Approach B — file-level split with a binary facade
- **Baseline:** `main @ bf8e395`, 489 tests passing

This plan decomposes the design into ordered, mechanically testable sub-tasks. Each sub-task builds on the previous and leaves the binary test suite green.

## Sub-tasks

### ST1 — Add `debug` module to the library target
**Files:** `src/lib.rs`
**Acceptance:** `cargo build --no-default-features` succeeds AND `cargo test` still passes (489 tests).
**Estimate:** S

Add `#[doc(hidden)] pub mod debug;` to `src/lib.rs` so that `src/provider/http.rs` can resolve `crate::debug_log!` when compiled as part of the library. Leave the binary's `mod debug;` in `src/main.rs` untouched — `src/debug.rs` is self-contained and can be compiled in both crates without type-identity issues.

### ST2 — Extract provider identity types into a dedicated library submodule
**Files:** `src/provider/mod.rs`, `src/provider/identity.rs`
**Acceptance:** `cargo test` passes (489 tests).
**Estimate:** M

Create `src/provider/identity.rs` and move the following verbatim from `src/provider/mod.rs`:

- `ProviderError` struct and `impl ProviderError`
- `ErrorKind` enum
- `ProviderId` enum (replace unconditional `#[derive(ValueEnum)]` with `#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]` and `#[cfg_attr(feature = "clap", value(rename_all = "lower"))]`)
- `AuthMethod` enum
- `ModelSource` enum
- `resolve_model_with_source`
- `env_u64`, `is_retryable`, `retry_after_hint`
- A new clap-free `ProviderId::parse` that honors `gemini` → `Google` and `google-vertex` → `Vertex` aliases.

Update `src/provider/mod.rs` to:

- Declare `pub mod identity;`
- Re-export the identity types: `ProviderId`, `ProviderError`, `ErrorKind`, `AuthMethod`, `ModelSource`, `resolve_model_with_source`
- Keep all binary-only content (`Provider` trait, `select()`, conflict types, per-provider modules, prompts, resolve helpers, vertex ADC probes) in place.

No changes to `src/lib.rs`, `src/main.rs`, or other binary modules in this step.

### ST3 — Move `http.rs` behind the library boundary with cfg gating
**Files:** `src/provider/http.rs`, `src/provider/mod.rs`
**Acceptance:** `cargo test` passes (489 tests).
**Estimate:** M

In `src/provider/http.rs`:

- Widen `HttpRequest`, `HttpGet`, `post_json`, `get_json`, `bad_request_detail`, `truncate` from `pub(super)` to `pub`.
- Gate `RetryConfig`, `retry_with`, `send_once`, `get_once`, `map_ureq_error`, `timeout_secs`, `post_json`, and `get_json` behind `#[cfg(feature = "cli")]`.
- Keep `debug_log!` calls as-is; ST1 made them resolvable in the library.
- Update internal imports (`use super::{env_u64, is_retryable, retry_after_hint, ErrorKind, ProviderError}` must resolve through the new identity module).

In `src/provider/mod.rs`:

- Declare `pub mod http;` (still inside the binary's provider module at this stage).
- Update re-exports if needed.

### ST4 — Move `models.rs` behind the library boundary with cfg gating
**Files:** `src/provider/models.rs`, `src/provider/mod.rs`
**Acceptance:** `cargo test` passes (489 tests).
**Estimate:** M

In `src/provider/models.rs`:

- Import `ProviderId`, `ProviderError` from `super::identity` (or via `super` re-exports).
- Make `fetch_supported_models_with` a `pub fn` (the library surface).
- Gate the convenience wrapper `fetch_supported_models` behind `#[cfg(feature = "cli")]` (it calls `http::get_json`, which needs `ureq`).
- Keep `FetchSource`, `ModelFetchOutcome`, and helpers as `pub`.

In `src/provider/mod.rs`:

- Declare `pub mod models;`.
- Re-export `FetchSource` and `fetch_supported_models` with the correct cfg gates.

### ST5 — Split binary facade from library module and wire crate roots
**Files:** `src/provider/mod.rs`, `src/provider/facade.rs`, `src/main.rs`, `src/lib.rs`, `src/provider/groq.rs`, `src/provider/gemini.rs`, `src/provider/openai.rs`, `src/provider/anthropic.rs`, `src/provider/ollama.rs`, `src/provider/vertex.rs`
**Acceptance:** `cargo test` passes AND `cargo build --no-default-features` succeeds.
**Estimate:** L

Create `src/provider/facade.rs` from the current binary-only content of `src/provider/mod.rs`:

- Remove identity/http/models definitions and replace with `pub use gcm::provider::{...}` re-exports and `pub use gcm::provider::http;`.
- Keep the `Provider` trait, `select()`, `pick_provider_id`, `resolve_model`, `ConflictHunk`, `ResolveContext`, `Resolution`, shared prompts, resolve helpers, vertex ADC probes, and all `pub(crate) mod groq/gemini/openai/anthropic/ollama/vertex;` declarations.

Rewrite `src/provider/mod.rs` to a pure library module:

```rust
pub mod identity;
pub mod http;
pub mod models;

pub use identity::{AuthMethod, ErrorKind, ModelSource, ProviderError, ProviderId, resolve_model_with_source};
pub use models::{fetch_supported_models_with, FetchSource, ModelFetchOutcome};
#[cfg(feature = "cli")]
pub use models::fetch_supported_models;
```

In `src/main.rs`:

- Change `mod provider;` to `#[path = "provider/facade.rs"] mod provider;`.

In `src/lib.rs`:

- Add `pub mod provider;` (now resolves to the library module).

Update per-provider files if needed so `use super::http::{...}` continues to resolve through the facade's `pub use gcm::provider::http;`.

### ST6 — Add library unit and integration tests
**Files:** `src/provider/identity.rs`, `src/provider/models.rs`, `tests/library_provider_api.rs`
**Acceptance:** `cargo test --no-default-features` passes AND `cargo test` passes.
**Estimate:** M

In `src/provider/identity.rs`, add unit tests covering:

- `resolve_model_with_source` precedence (flag > env > default).
- Empty/whitespace CLI and env values are skipped.
- `ProviderId::parse` accepts all providers and aliases case-insensitively.
- `ProviderId::parse` rejects unknown names.
- `ProviderId::key_env_var` returns `None` for Ollama/Vertex, `Some` for others.
- `ProviderId::auth_method` returns the correct variant for each provider.

In `src/provider/models.rs`, add unit tests covering:

- `fetch_supported_models_with` with an injected fetcher returning the static fallback body → `FetchSource::Fallback`.
- `fetch_supported_models_with` with an injected fetcher returning a live body → `FetchSource::Live` with expected filtered/deduped models.
- `fetch_supported_models_with` with an injected fetcher returning `Err(ProviderError)` → degrades to fallback with a warning.
- Missing key for a key-bearing provider → fallback with key warning.
- Missing ADC for Vertex → fallback with ADC warning.

Create `tests/library_provider_api.rs`:

- Import `gcm::provider::{ProviderId, resolve_model_with_source, ModelSource}` and resolve a model with a caller-supplied env map.
- Import `gcm::provider::models::fetch_supported_models_with` and drive it with an injected fetcher that returns the static fallback.

### ST7 — Finalize formatting, lints, and documentation
**Files:** `src/lib.rs`, `src/provider/*.rs`, `src/provider/facade.rs`
**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` passes.
**Estimate:** S

- Run `cargo fmt` and fix any formatting issues.
- Run `cargo clippy --all-targets -- -D warnings` and resolve all warnings.
- Add top-level doc comments to `src/lib.rs` and `src/provider/mod.rs` explaining the library/binary split and pointing to ADR-002.
- Add a short comment above `#[doc(hidden)] pub mod http;` explaining that `pub` visibility is required for binary access but the module is not part of the public API.
- Verify no dead code or unused imports were introduced by the refactor.

## Pre-merge gate

- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Per-provider files accidentally resolve to duplicate types if the facade misses a `pub use` re-export. | High (compilation or logic break) | ST5 acceptance includes `cargo build --no-default-features`, which exercises the library type definitions independently. Full `cargo test` confirms binary integration. |
| Hand-rolled `ProviderId::parse` diverges from `clap::ValueEnum::from_str` alias behavior. | Medium (CLI `--provider` alias regression) | ST2 and ST6 include unit tests for all documented aliases (`gemini`, `google-vertex`) and unknown inputs. Manual HITL tests verify `--provider gemini` and `--provider google-vertex`. |
| `http.rs` visibility widened to `pub` exposes internals in public docs. | Low | Module is declared as `#[doc(hidden)] pub mod http;` in the library. Items are intended for binary consumption and will not appear in `rustdoc`. |
| `debug.rs` compiled in both crates introduces duplicate symbols or macro conflicts. | Low | `src/debug.rs` is self-contained; `#[macro_export]` macros expand to `$crate::debug::...`, which resolves correctly in each crate. ST1 acceptance runs both `cargo build --no-default-features` (library) and `cargo test` (binary). |
| `fetch_supported_models` gated behind `cli` breaks external callers expecting it with no default features. | Low | ADR-002 restricts the library to no-transport types. The always-available surface is `fetch_supported_models_with`; the convenience wrapper is a `cli`-only convenience. Documented in the design. |

## Notes

- `Cargo.toml` requires no changes; the feature setup from CLO-594/CLO-595 is already correct.
- Files outside `src/provider/` and `src/main.rs`/`src/lib.rs` should not need changes because the facade re-exports library types transparently.
- If any sub-task reveals that the design is incomplete, return to the design phase for user confirmation rather than patching ad-hoc.
