# ADR-002: gcm Library Boundary — Crate Shape, Sync/Async Seam, and Config Boundary

**Status:** Accepted
**Date:** 2026-07-27
**Linear Task:** [CLO-594](https://linear.app/cloud-ai/issue/CLO-594) — Lock the gcm library boundary, the sync/async seam and the config shape (ADR)
**Design Doc:** N/A (this ADR is the design artifact; driven by [discovery report](../discovery/clo-594.md))
**Predecessor:** [CLO-593](https://linear.app/cloud-ai/issue/CLO-593) — Extract lok's Backend abstraction into a consumable library target
**Unblocks:** [CLO-595](https://linear.app/cloud-ai/issue/CLO-595) — Ship gcm's secret scanner as a library API consumable by other crates

## Context

gcm is a 18,830-line Rust CLI with a single `[[bin]]` target and no `src/lib.rs`. Every module — config, provider, privacy, plan, diff, resolve — is internal to the binary. Two downstream consumers need a library boundary:

1. **lok** (async, tokio+reqwest) needs gcm's secret scanner and config types.
2. A **remem-ai** memory pipeline fork needs the same.

Today there is no library boundary at all: importing any gcm type drags in `cliclack` (the interactive wizard), `clap` (CLI argument parsing), and `ureq` (sync HTTP). The three decisions — crate shape, sync vs async, config seam — are interdependent and must be locked before any extraction code moves.

Three facts frame the whole set:

1. **The binary is a daily-use signed-commit tool.** The primary user (Max) runs `gcm` multiple times per day. Any disruption to the build, the branch layout, or the commit flow is immediately felt. The extraction must be surgical, not a rewrite.
2. **The shared surface is types, not transport.** lok already has its own async HTTP transport (tokio+reqwest). The value of sharing gcm's code is in the *types* — `Config`, `ProviderId`, `SecretScanMode` — and the pure functions that operate on them, not in the HTTP client or the provider backends.
3. **CLO-485 set the precedent.** A single ADR ticket (CLO-485) locked 13 foundational decisions before any code moved. This ADR follows the same pattern: lock the boundary decisions before any extraction slice begins.

## Decision Summary

| # | Question | Decision | FR |
|---|----------|----------|-----|
| 1 | Crate shape | **[lib] target inside existing package** (not a workspace) | — |
| 2 | Sync vs async | **gcm stays sync; type boundary is the seam** | — |
| 3 | Config seam | **Data types + pure functions in library; wizard in binary** | — |
| 4 | `clap` dependency | **Optional `clap` feature on the library** | — |
| 5 | `Provider` trait | **Stays in binary** (confirmed per issue scope) | — |
| 6 | Wizard | **Stays in binary** | — |
| 7 | Registry | **Path-only for now** (no publish until at least one consumer exists) | — |

---

## Decision 1 — Crate shape: [lib] target inside existing package

**Decision:** Add a `[lib]` target to the existing `Cargo.toml` that exposes a curated subset of types. The library and binary share versioning and one registry entry. Do **not** create a workspace with a separate crate.

**Drivers:**
1. **Minimal disruption** — the primary user's daily-use tool keeps its `Cargo.toml`, its `cargo build` workflow, and its `src/` layout. No workspace root, no path-dependency gymnastics, no `use gcm_core::X` migration across the entire codebase.
2. **Single version, single publish** — one `cargo publish` updates both the library and the binary. No version-sync headaches between two crates.
3. **Incremental extraction** — the `[lib]` target can start with a small re-export surface (`pub use config::Config`, `pub use provider::ProviderId`, `pub use privacy::SecretScanMode`) and grow as more types are extracted. The binary continues to use `use crate::config::Config` internally; the library re-exports via `pub use`.

**Alternatives considered:**
- *Workspace with separate crate (rejected):* clean semantic boundary and independent versioning, but forces every `use crate::X` in the binary to become `use gcm_core::X` or a path dependency. This is a daily-use signed-commit tool — the disruption to the primary user's workflow is real and immediate. Two `Cargo.toml` files, two `cargo publish` steps, two version numbers to track. The workspace root changes the `cargo` UX for existing contributors.
- *No library at all (rejected):* every consumer forks or duplicates the types they need. This is the status quo and the problem this ADR exists to solve.

**Consequences:**
- (+) Minimal disruption: no workspace, no new `Cargo.toml`, no git-subtree or path-dependency gymnastics.
- (+) Single `cargo publish` — one version number, one registry entry.
- (+) The binary continues to use `use crate::config::Config` internally; the library re-exports via `pub use`.
- (−) Library versioning is tied to the binary — a breaking change in the library forces a binary version bump.
- (−) The `[lib]` target shares the same `Cargo.toml` dependency set; optional features (`clap` as optional) need `[features]` wiring.
- (−) Binary-internal modules (`cache`, `git`, `resolve`, `ui`) must be gated behind `#[cfg(not(feature = "library"))]` or kept as private implementation details.
- (→) **Planned evolution:** if independent versioning becomes necessary (e.g., the library has 3+ consumers with different release cadences), split into a workspace. The `[lib]` target makes this a mechanical move: create a new crate, copy the `[lib]` code, add a path dependency. Contained behind the library boundary, not a v1 commitment.

---

## Decision 2 — Sync vs async: gcm stays sync; type boundary is the seam

**Decision:** gcm's transport remains sync (`ureq`). Only non-transport types cross the library boundary. The shared surface is `struct`/`enum` definitions and pure functions that do I/O only through injected dependencies.

**Drivers:**
1. **gcm's transport is sync `ureq`** — rewriting it to async would touch every provider backend (Groq, Google, OpenAI, Anthropic, Ollama, Vertex) and the `fetch_supported_models` live HTTP path at `src/provider/models.rs:39`. This is a large, risky refactor of a daily-use tool with no immediate benefit.
2. **lok already has its own transport** — lok's `Backend` is async over tokio and reqwest. The shared surface is the *types* (config, secret scan mode, provider ID), not the transport implementation. An async consumer can use sync types without any async coloring.
3. **The type boundary is sync-agnostic** — `Config`, `ProviderId`, `SecretScanMode`, and the pure functions that operate on them (`load`, `save`, `apply_to_env`) are plain `struct`/`enum` definitions and `fn` that do I/O only through injected dependencies. They have no async methods, no `Future` return types, and no `Send`/`Sync` constraints.

**Alternatives considered:**
- *gcm transport becomes an implementation behind lok's async trait (rejected):* premature coupling. gcm and lok have different transport needs (gcm: single sync call per invocation; lok: streaming, multi-provider concurrency). Forcing gcm's transport behind lok's async trait would require an async adapter layer with no consumer.
- *Both converge on async (rejected):* rewrites gcm's runtime model from scratch. Every provider backend, every HTTP call, every test — all become async. This is a multi-week refactor of a daily-use tool with no immediate benefit to either consumer.

**Consequences:**
- (+) No rewrite of gcm's transport or provider backends.
- (+) The type boundary is clean and sync-agnostic — any consumer, sync or async, can use it.
- (+) Future convergence is not ruled out — if a later phase needs shared transport (e.g., a unified HTTP client with retry logic), that can be extracted behind a trait.
- (−) An async consumer that wants to share transport code (not just types) must write its own adapter. This is acceptable because lok already has its own transport.
- *Condition for revisiting:* if two consumers share the same transport implementation (same HTTP client, same retry logic, same error taxonomy), extract it behind a trait. Until then, the type boundary is sufficient.

---

## Decision 3 — Config seam: data types + pure functions in library; wizard in binary

**Decision:** The library exports `Config`, `ProviderConfig`, `ConflictConfig`, `AutoPolicy`, and the pure functions (`load`, `save`, `apply_to_env`, `config_path`, `needs_onboarding`). The wizard functions (`run_wizard`, `run_provider_wizard`) and `non_tty_instructions` stay in the binary because they depend on `cliclack` and `console`.

**Drivers:**
1. **`src/config.rs` is 2,530 lines** — the largest file in the crate. Moving `Config` wholesale would drag `cliclack` (the interactive wizard) into the library, which is unacceptable for a non-CLI consumer.
2. **The wizard is ~560 lines** of the config file (`run_wizard` at line 483, `run_provider_wizard` at line 791). These are the only functions that depend on `cliclack` and `console`.
3. **The pure functions are self-contained** — `load`, `save`, `apply_to_env`, `config_path`, `needs_onboarding` do I/O through `std::fs` and `std::env` only. They have no dependency on `cliclack`, `console`, or `clap`.

**Alternatives considered:**
- *Move `Config` wholesale (rejected):* drags `cliclack` into the library. A non-CLI consumer (e.g., a background service) would have an unused interactive wizard in its dependency tree.
- *Split `config.rs` into library + binary files (rejected for now):* cleaner file layout, but adds churn to a daily-use tool. The `#[cfg(not(feature = "library"))]` gate on the wizard functions is sufficient for v1. A file split can happen when the library surface stabilizes.

**Consequences:**
- (+) The library has zero dependency on `cliclack` or `console`.
- (+) The binary continues to use `use crate::config::Config` internally; the library re-exports via `pub use`.
- (−) The wizard functions live in the same file as the library types, gated behind `#[cfg(not(feature = "library"))]`. This is a minor readability cost.
- (→) **Planned evolution:** split `config.rs` into `config/mod.rs` (library types + pure functions) and `config/wizard.rs` (binary-only wizard) when the library surface stabilizes.

---

## Decision 4 — `clap` dependency: optional feature on the library

**Decision:** The library has an optional `clap` feature. When enabled, `SecretScanMode` and `ProviderId` derive `clap::ValueEnum` via `#[cfg_attr(feature = "clap", derive(ValueEnum))]`. When disabled, they derive only `Serialize`/`Deserialize`/`Debug`/`Clone`/`Copy`/`PartialEq`/`Eq`.

**Drivers:**
1. **Two types derive `clap::ValueEnum`** — `SecretScanMode` at `src/privacy/mod.rs:13` and `ProviderId` at `src/provider/mod.rs:330`. Both are needed by the library, but `clap` is a CLI-only concern.
2. **A non-CLI consumer should not depend on `clap`** — a background service or async consumer that imports `gcm-core` should not have `clap` in its dependency tree.
3. **The `cfg_attr` pattern is idiomatic Rust** — used throughout the ecosystem (e.g., `serde`'s `#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]`).

**Alternatives considered:**
- *Binary carries wrappers (rejected):* the binary would define its own `ValueEnum` wrappers around the library types, requiring conversion code and breaking the direct `--secret-scan`/`--provider` flag binding.
- *Library always depends on `clap` (rejected):* forces `clap` on every consumer, even non-CLI ones. `clap` is a large dependency (derive macros, proc-macro2, syn, quote).

**Consequences:**
- (+) Non-CLI consumers have zero `clap` dependency.
- (+) The binary enables the `clap` feature and gets `ValueEnum` derives for free.
- (+) The `cfg_attr` pattern is well-understood and used throughout the Rust ecosystem.
- (−) Every `ValueEnum` derive needs `#[cfg_attr(feature = "clap", derive(ValueEnum))]` — two sites to maintain.
- (−) The test matrix expands: `cargo test --no-default-features` must pass.

---

## Decision 5 — `Provider` trait: stays in binary

**Decision:** The `Provider` trait and all its implementations (Groq, Google, OpenAI, Anthropic, Ollama, Vertex) stay in the binary. The library does not export the trait.

**Drivers:**
1. **The trait imports `crate::plan::Plan` and `crate::diff::{DiffBudget, GatheredDiff, GroupingContext}`** (line 30-31 of `src/provider/mod.rs`). These are binary-internal domain types that encode commit grouping and merge-conflict resolution.
2. **Exporting the trait would force unrelated domain types on every consumer** — a consumer that only wants `SecretScanMode` should not need to understand `Plan`, `GatheredDiff`, or `GroupingContext`.
3. **The issue explicitly rejects this** — "Rejected already, do not revisit: exporting gcm's `Provider` trait."

**Alternatives considered:**
- *Export the trait (rejected):* forces `plan` and `diff` types on every consumer. These types are specific to gcm's commit-grouping domain and have no meaning for a secret-scanner consumer.
- *Extract a minimal `HttpClient` trait (rejected for now):* if a later phase needs shared transport, a minimal HTTP trait could be extracted. But no consumer needs it today.

**Consequences:**
- (+) The library surface stays small and focused on types.
- (+) No `plan` or `diff` types leak into the library.
- (−) A consumer that wants to use gcm's provider backends must depend on the binary crate. This is acceptable because the provider backends are gcm-specific and not intended for external reuse.

---

## Decision 6 — Wizard: stays in binary

**Decision:** The interactive wizard functions (`run_wizard`, `run_provider_wizard`) stay in the binary. The library does not export them.

**Drivers:**
1. **The wizard depends on `cliclack` and `console`** — these are interactive TTY libraries that have no place in a library consumed by background services or async runtimes.
2. **The wizard is a binary-only concern** — it is invoked by `gcm config` and the first-run onboarding flow. A library consumer configures gcm programmatically, not through an interactive TTY prompt.

**Alternatives considered:**
- *Move wizard to library (rejected):* drags `cliclack` and `console` into the library dependency tree.
- *Remove wizard (rejected):* breaks the onboarding UX that CLO-496 and CLO-516 implemented.

**Consequences:**
- (+) The library has zero dependency on `cliclack` or `console`.
- (+) The wizard continues to work exactly as it does today.
- *Condition for revisiting:* if a consumer needs programmatic config generation (not interactive), the pure `Config` constructors and `save` function in the library are sufficient. No wizard export is needed.

---

## Decision 7 — Registry: path-only for now

**Decision:** The library is consumed by path dependency only (`gcm-core = { path = "../gcm" }` or similar). It is not published to crates.io until at least one external consumer exists.

**Drivers:**
1. **No external consumers exist yet** — the only consumers are lok and remem-ai, both in the same monorepo/organization. Publishing to crates.io adds maintenance burden (version bumps, changelog, semver policy) with no benefit.
2. **Path dependencies are sufficient for in-org consumers** — lok can depend on `gcm-core` via a path or git dependency. Publishing is only needed when a consumer outside the organization wants to use it.

**Alternatives considered:**
- *Publish to crates.io (rejected):* premature. No consumers outside the organization exist. Publishing adds semver maintenance, CI publish steps, and version bump overhead.

**Consequences:**
- (+) No publish maintenance until at least one external consumer exists.
- (+) Path dependencies are simple and well-understood.
- (−) In-org consumers must use path or git dependencies. This is acceptable for a monorepo.
- *Condition for revisiting:* when the first external consumer requests a crates.io publish, add a publish CI step and semver policy.

---

## Library Surface (Summary)

After extraction, the library (`gcm-core` via `[lib]` in the existing package) exports:

```
gcm-core (via [lib] target):
  config::Config
  config::ProviderConfig
  config::ConflictConfig
  config::AutoPolicy
  config::load
  config::save
  config::apply_to_env
  config::config_path
  config::needs_onboarding
  provider::ProviderId
  provider::AuthMethod
  privacy::SecretScanMode
```

The binary (`main.rs`) retains:

```
Binary-only (not in library):
  config::run_wizard
  config::run_provider_wizard
  config::non_tty_instructions
  provider::Provider trait + all backends (Groq, Google, OpenAI, Anthropic, Ollama, Vertex)
  cache, cli, debug, diff, git, output, paths, plan, resolve, status, ui
```

## Consequences

1. **The library is publishable independently** — it has no dependency on `cliclack`, `console`, or `clap` (unless the consumer opts in via the `clap` feature).
2. **The binary is unchanged** — all `use crate::X` imports continue to work. The `[lib]` target is additive.
3. **The extraction is incremental** — start with a small re-export surface, grow as more types are extracted. The first extraction slice (CLO-595: secret scanner) only needs `SecretScanMode` and `Config`.
4. **The ADR is the design artifact** — the implementation tasks (CLO-595+) reference this ADR for the boundary decisions.

## Implementation Notes (from Gemini design review)

The following items were identified by the Gemini 3.5 Flash design review (2026-07-27) and are incorporated as implementation guidance for CLO-595+:

1. **Type identity (critical):** In a single-package `[lib]+[[bin]]` setup, the binary must import shared types from the library crate (`use gcm::config::Config`), not declare `mod config;` in both targets. Otherwise the compiler treats `crate::config::Config` (binary) and `gcm::config::Config` (library) as distinct, incompatible types.
2. **Optional dependencies:** `cliclack` and `console` must be `optional = true` in `Cargo.toml` and gated behind a `cli` feature (enabled by default for the binary). Without this, every library consumer inherits them as non-optional dependencies.
3. **Module gating is unnecessary:** Binary-internal modules (`cache`, `git`, `resolve`, `ui`) do not need `#[cfg(not(feature = "library"))]` — omitting them from `lib.rs` is sufficient to exclude them from the library build.
4. **Platform-conditional permissions:** The `save` function's `0600` permission logic must use `#[cfg(unix)]` conditional compilation to avoid breaking Windows builds.
5. **Error type decoupling:** `GcmError` contains binary-specific variants (`Git`, `Provider`, `OnboardingRequired`). The first extraction (CLO-595) only exports `SecretScanMode` (no `GcmError` return), so this is deferred. If a later extraction exports functions that return `GcmError`, the binary-specific variants must be feature-gated or a library-specific error type must be defined.

## References

- [CLO-594](https://linear.app/cloud-ai/issue/CLO-594) — Lock the gcm library boundary (this task)
- [Discovery Report](../discovery/clo-594.md) — Problem framing, code exploration, approach analysis
- [ADR-001](001-foundational-architecture-decisions.md) — Foundational architecture decisions (precedent for single-ADR locking)
- [CLO-593](https://linear.app/cloud-ai/issue/CLO-593) — Extract lok's Backend abstraction (predecessor)
- [CLO-595](https://linear.app/cloud-ai/issue/CLO-595) — Ship gcm's secret scanner as a library API (first consumer)
