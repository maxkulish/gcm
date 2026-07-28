# PRD: CLO-596 — Expose provider identity and the live model registry through the gcm library

**Status:** Draft (discovery phase)  
**Source:** [CLO-596](https://linear.app/cloud-ai/issue/CLO-596/expose-provider-identity-and-the-live-model-registry-through-the-gcm)  
**Depends on:** [CLO-594](https://linear.app/cloud-ai/issue/CLO-594) (library boundary ADR), [CLO-595](https://linear.app/cloud-ai/issue/CLO-595) (scanner extraction precedent)  
**Unblocks:** [CLO-597](https://linear.app/cloud-ai/issue/CLO-597) (status resolution library surface)

## Goal

Any tool that offers a model picker — the `gcm` CLI itself, a future IDE plugin, or an in-org consumer such as `lok` — should be able to reuse gcm's six-provider identity layer and its live model registry with static fallbacks, instead of rebuilding provider names, key env vars, model lists, and alias handling from scratch.

## Non-Goals

- Extract the commit-shaped `Provider` trait or any of its six backend implementations (Groq, Google, OpenAI, Anthropic, Ollama, Vertex). They remain binary-only.
- Extract the `gcm resolve` conflict types (`ConflictHunk`, `ResolveContext`, `Resolution`). They remain binary-only.
- Publish to crates.io. The library is consumed via path dependency only until an external consumer exists (per ADR-002 Decision 7).

## Context

`src/provider/mod.rs` is the largest provider file and interleaves two unrelated audiences:

| Library side | ~Line | Binary side | ~Line |
| -- | -- | -- | -- |
| `ProviderError` | 58 | `Provider` trait | 36 |
| `ErrorKind` | 66 | `ConflictHunk` | 156 |
| `ProviderId` | 333 | `ResolveContext` | 167 |
| `AuthMethod` | 350 | `Resolution` | 178 |
| `ModelSource` | 517 | `select()` | 433 |
| `resolve_model_with_source` | 532 | per-provider impls | `groq.rs` and siblings |

`src/provider/models.rs` and `src/provider/http.rs` carry no `crate::plan`/`crate::diff` references and are already reusable in spirit, but `models.rs` depends on `super::ProviderId` and `super::ProviderError`. Until those cross into the library, the registry cannot cross either.

`ProviderId` currently derives `clap::ValueEnum` unconditionally. ADR-002 Decision 4 mandates an optional `clap` feature so non-CLI consumers do not inherit `clap`.

## Scope

1. Split `src/provider/mod.rs` so the library exports identity/registry types and the binary keeps the commit-domain trait and conflict types.
2. Move `src/provider/http.rs` and `src/provider/models.rs` behind the library boundary (binary still uses them via the library target).
3. Resolve `ProviderId`'s `clap` dependency per ADR-002 (optional `clap` feature, `cfg_attr` derive).
4. Keep `Provider`, `select()`, `ConflictHunk`, `ResolveContext`, and `Resolution` out of the library's public surface.
5. Add library unit tests that exercise the new public API with injected dependencies and no network.

## Acceptance Criteria

- [ ] 475 tests still pass (baseline today is 489), with `tests/provider.rs` and `tests/vertex.rs` unchanged in intent.
- [ ] A library unit test resolves a model and its `ModelSource` through `gcm::provider` using a caller-supplied env map (no `std::env::var`).
- [ ] A library unit test drives `fetch_supported_models_with` against an injected fetcher and receives the static fallback, touching no network.
- [ ] `--provider groq` and the `gemini` / `google-vertex` aliases are still accepted by the binary CLI.
- [ ] `gcm provider` still lists live Vertex and Google models, so CLO-564 behavior is intact.
- [ ] `cargo test --no-default-features` compiles the library provider surface.

## Risks

- `ProviderId` is serialized in `config.toml` via serde and parsed on the CLI via clap. Moving it across the `[lib]`/`[[bin]]` seam while preserving type identity is mechanical but easy to get wrong if both crates declare a `ProviderId`.
- The model registry's default fetch path (`fetch_supported_models`) uses `ureq` via `http::get_json`. Keeping that dependency binary-side while exposing the pure `fetch_supported_models_with` helper requires careful `cfg` gating or moving the default into the binary module.
- `http.rs` currently logs via `crate::debug_log!`, which lives in a binary-only module. Either `debug` must become library-reachable or the calls must be replaced with a library-safe log hook.
