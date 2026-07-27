# ADR-002: gcm Library Boundary — Crate Shape, Sync/Async Seam, and Config Boundary

**Status:** Accepted, surface amended 2026-07-27 (see [Amendment](#amendment-2026-07-27-library-surface-widened))
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
3. **Incremental extraction** — the `[lib]` target can start with a small re-export surface (`pub use config::Config`, `pub use provider::ProviderId`, `pub use privacy::SecretScanMode`) and grow as more types are extracted. The binary imports shared types from the library target (`use gcm::config::Config`) to ensure type identity — the `[lib]` and `[[bin]]` targets compile as separate crates, so `crate::Config` and `gcm::Config` are distinct, incompatible types if both declare `mod config;`. Binary-internal modules (`cache`, `git`, `resolve`, `ui`) stay as local `mod` declarations in `main.rs` only; the library omits them from `lib.rs`.

**Alternatives considered:**
- *Workspace with separate crate (rejected):* clean semantic boundary and independent versioning, but forces every `use crate::X` in the binary to become `use gcm_core::X` or a path dependency — *every* module, not just the shared types. This is a daily-use signed-commit tool — the disruption to the primary user's workflow is real and immediate. Two `Cargo.toml` files, two `cargo publish` steps, two version numbers to track. The workspace root changes the `cargo` UX for existing contributors. Note: the chosen `[lib]` option still requires migrating shared-type imports (`use gcm::config::Config`), but binary-internal modules keep their `mod` declarations — far less churn.
- *No library at all (rejected):* every consumer forks or duplicates the types they need. This is the status quo and the problem this ADR exists to solve.

**Consequences:**
- (+) Minimal disruption: no workspace, no new `Cargo.toml`, no git-subtree or path-dependency gymnastics.
- (+) Single `cargo publish` — one version number, one registry entry.
- (+) The binary imports shared types from the library target (`use gcm::config::Config`); binary-internal modules stay as local `mod` declarations.
- (−) Library versioning is tied to the binary — a breaking change in the library forces a binary version bump.
- (−) The `[lib]` target shares the same `Cargo.toml` dependency set; optional features (`clap` as optional) need `[features]` wiring.
- (−) Binary-internal modules (`cache`, `git`, `resolve`, `ui`) must be gated behind `#[cfg(not(feature = "library"))]` or kept as private implementation details.
- (→) **Planned evolution:** if independent versioning becomes necessary (e.g., the library has 3+ consumers with different release cadences), split into a workspace. The `[lib]` target makes this a mechanical move: create a new crate, copy the `[lib]` code, add a path dependency. Contained behind the library boundary, not a v1 commitment.

---

## Decision 2 — Sync vs async: gcm stays sync; type boundary is the seam

**Decision:** gcm's transport remains sync (`ureq`). No async runtime, and no `Future`-returning method, crosses the library boundary. The shared surface is `struct`/`enum` definitions plus functions whose I/O is reachable through an injected dependency, so a caller can drive them without going through gcm's HTTP client.

Transport *implementations* stay in the binary: that means the six provider backends, which speak each vendor's wire format behind the commit-shaped `Provider` trait. A sync helper that accepts an injectable fetcher is not a transport implementation and may cross. `fetch_supported_models_with` at `src/provider/models.rs` already takes `fetch: impl Fn(&HttpGet) -> Result<String, ProviderError>`, which is exactly the shape this allows, so the model registry qualifies (amended 2026-07-27).

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
- (+) The binary imports `Config` from the library target (`use gcm::config::Config`); the wizard functions stay as local `mod` declarations in `main.rs`.
- (−) The wizard functions live in the same file as the library types, gated behind `#[cfg(not(feature = "library"))]`. This is a minor readability cost.
- (→) **Planned evolution:** split `config.rs` into `config/mod.rs` (library types + pure functions) and `config/wizard.rs` (binary-only wizard) when the library surface stabilizes.

---

## Decision 4 — `clap` dependency: optional feature on the library

**Decision:** The library has an optional `clap` feature. When enabled, `SecretScanMode`, `ProviderId`, and `AutoPolicy` derive `clap::ValueEnum` via `#[cfg_attr(feature = "clap", derive(ValueEnum))]`. When disabled, they derive only `Serialize`/`Deserialize`/`Debug`/`Clone`/`Copy`/`PartialEq`/`Eq`.

**Drivers:**
1. **Three types derive `clap::ValueEnum`.** `SecretScanMode` at `src/privacy/mod.rs:13`, `ProviderId` at `src/provider/mod.rs:333`, and `AutoPolicy` at `src/config.rs:140`. All three are needed by the library, but `clap` is a CLI-only concern.
2. **A non-CLI consumer should not depend on `clap`** — a background service or async consumer that imports the library should not have `clap` in its dependency tree.
3. **The `cfg_attr` pattern is idiomatic Rust** — used throughout the ecosystem (e.g., `serde`'s `#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]`).

**Alternatives considered:**
- *Binary carries wrappers (rejected):* the binary would define its own `ValueEnum` wrappers around the library types, requiring conversion code and breaking the direct `--secret-scan`/`--provider` flag binding.
- *Library always depends on `clap` (rejected):* forces `clap` on every consumer, even non-CLI ones. `clap` is a large dependency (derive macros, proc-macro2, syn, quote).

**Consequences:**
- (+) Non-CLI consumers have zero `clap` dependency.
- (+) The binary enables the `clap` feature and gets `ValueEnum` derives for free.
- (+) The `cfg_attr` pattern is well-understood and used throughout the Rust ecosystem.
- (−) Every `ValueEnum` derive needs `#[cfg_attr(feature = "clap", derive(ValueEnum))]` — three sites to maintain.
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

**Decision:** The library is consumed by path dependency only (`gcm = { path = "../gcm" }` or similar). It is not published to crates.io until at least one external consumer exists.

**Drivers:**
1. **No external consumers exist yet** — the only consumers are lok and remem-ai, both in the same monorepo/organization. Publishing to crates.io adds maintenance burden (version bumps, changelog, semver policy) with no benefit.
2. **Path dependencies are sufficient for in-org consumers** — lok can depend on the library via a path or git dependency. Publishing is only needed when a consumer outside the organization wants to use it.

**Alternatives considered:**
- *Publish to crates.io (rejected):* premature. No consumers outside the organization exist. Publishing adds semver maintenance, CI publish steps, and version bump overhead.

**Consequences:**
- (+) No publish maintenance until at least one external consumer exists.
- (+) Path dependencies are simple and well-understood.
- (−) In-org consumers must use path or git dependencies. This is acceptable for a monorepo.
- *Condition for revisiting:* when the first external consumer requests a crates.io publish, add a publish CI step and semver policy.

---

## Library Surface (Summary)

After extraction, the library (via `[lib]` in the existing package) exports:

```
gcm (via [lib] target):

  # Config types and pure functions
  config::Config
  config::ProviderConfig
  config::ConflictConfig
  config::AutoPolicy
  config::load
  config::save
  config::apply_to_env
  config::config_path
  config::needs_onboarding

  # Secret scanner (CLO-595)
  privacy::SecretScanMode              # resolved via an env_lookup closure, not process env
  privacy::rules::RuleEngine
  privacy::rules::CompiledRule
  privacy::rules::vendored             # compiles the embedded rules.toml corpus
  privacy::detect::secret_ranges
  privacy::detect::redact_secrets
  privacy::detect::merge_ranges
  privacy::entropy::Charset
  privacy::entropy::shannon_entropy
  privacy::entropy::normalized_entropy

  # Provider identity and model registry (CLO-596)
  provider::ProviderId
  provider::AuthMethod
  provider::ModelSource
  provider::resolve_model_with_source
  provider::ProviderError               # transport error, needed by the registry
  provider::ErrorKind
  provider::models::fetch_supported_models
  provider::models::ModelFetchOutcome
  provider::models::FetchSource

  # Source-attributed status resolution (CLO-597)
  status::StatusReport
  status::PathsStatus
  status::ProviderStatus
  status::<resolution entry point taking a config value plus env_lookup>
```

`src/privacy/rules.rs` embeds `rules.toml` through `include_str!`, so the corpus moves with the module.

The binary (`main.rs`) retains:

```
Binary-only (not in library):
  config::run_wizard
  config::run_provider_wizard
  config::non_tty_instructions
  privacy::Privacy                      # diff/git-shaped facade over the scanner
  provider::Provider trait + all backends (Groq, Google, OpenAI, Anthropic, Ollama, Vertex)
  provider::select                      # returns Box<dyn Provider>
  provider::ConflictHunk
  provider::ResolveContext
  provider::Resolution
  status::run_status_subcommand         # the Cli-shaped entry point
  cache, cli, debug, diff, git, output, paths, plan, resolve, ui
```

## Consequences

1. **The library is publishable independently** — it has no dependency on `cliclack`, `console`, or `clap` (unless the consumer opts in via the `clap` feature).
2. **The binary imports shared types from the library target** — the binary's shared-type imports (`Config`, `ProviderId`, `SecretScanMode`) change from `use crate::X` to `use gcm::X` to ensure type identity. Binary-internal modules (`cache`, `git`, `resolve`, `ui`) keep their `mod` declarations in `main.rs` and are omitted from `lib.rs`. The `[lib]` target is additive.
3. **The extraction is incremental.** Start with a small re-export surface, grow as more types are extracted. The first extraction slice (CLO-595: secret scanner) needs `SecretScanMode` plus the scanner itself: `RuleEngine`, `vendored`, `secret_ranges`, `redact_secrets` and the entropy helpers. A consumer given only the mode enum has nothing to scan with.
4. **The ADR is the design artifact** — the implementation tasks (CLO-595+) reference this ADR for the boundary decisions.

## Implementation Notes (from Gemini design review)

The following items were identified by the Gemini 3.5 Flash design review (2026-07-27) and are incorporated as implementation guidance for CLO-595+:

1. **Type identity (critical):** In a single-package `[lib]+[[bin]]` setup, the binary must import shared types from the library crate (`use gcm::config::Config`), not declare `mod config;` in both targets. Otherwise the compiler treats `crate::config::Config` (binary) and `gcm::config::Config` (library) as distinct, incompatible types.
2. **Optional dependencies:** `cliclack` and `console` must be `optional = true` in `Cargo.toml` and gated behind a `cli` feature (enabled by default for the binary). Without this, every library consumer inherits them as non-optional dependencies.
3. **Module gating is unnecessary:** Binary-internal modules (`cache`, `git`, `resolve`, `ui`) do not need `#[cfg(not(feature = "library"))]` — omitting them from `lib.rs` is sufficient to exclude them from the library build.
4. **Platform-conditional permissions:** The `save` function's `0600` permission logic must use `#[cfg(unix)]` conditional compilation to avoid breaking Windows builds.
5. **Error type decoupling:** `GcmError` contains binary-specific variants (`Git`, `Provider`, `OnboardingRequired`). The scanner functions CLO-595 exports do not return it (`secret_ranges` yields `Vec<Range<usize>>`, `redact_secrets` yields `String`, `vendored` yields `Result<_, String>`), so the decoupling is deferred rather than avoided. It comes due at the abort path: `Privacy::scan_text` signals a detection through `GcmError::SecretDetected { count }` at `src/privacy/mod.rs:93`, so CLO-595 must define a library-side error for that one case and have the binary map it back. Later extractions returning `GcmError` need the binary-specific variants feature-gated or a library error type defined.

## Amendment 2026-07-27: library surface widened

The seven decisions above are unchanged. What changed is the **Library Surface** section, which as first written locked a boundary narrower than the three extraction tickets it exists to unblock.

Three gaps, found by reading the surface list against each ticket's acceptance criteria:

1. **The scanner was missing.** The surface listed `privacy::SecretScanMode` alone, and the Consequences section stated CLO-595 "only needs `SecretScanMode` and `Config`". But `SecretScanMode` is the `off`/`redact`/`abort` enum; the scanner is `secret_ranges`, `redact_secrets`, `merge_ranges`, `RuleEngine`, `vendored` and the entropy helpers. CLO-595's acceptance criteria require an external consumer to compile a rule engine and scan text, which the original surface could not do. This is the gap that blocked the next ticket, so it drove the amendment.
2. **`status` was listed as binary-only**, which is precisely what CLO-597 exists to expose. The binary-side `run_status_subcommand` stays; the resolution path and report structs cross.
3. **The model registry was absent**, and Decision 2's "only non-transport types cross the boundary" read as ruling it out. Decision 2 is now explicit that an injectable-fetcher helper is not a transport implementation, which is the distinction that lets CLO-596 proceed while the six provider backends stay behind.

Also corrected: `ProviderId` cited at `mod.rs:330` is at `:333`, `AutoPolicy` cited at `config.rs:138` is at `:140`, and Implementation Note 5 understated the error work by overlooking `GcmError::SecretDetected` on the abort path.

The alternative considered was re-scoping CLO-596 and CLO-597 down to the original surface. Rejected: it would ship a library carrying types but neither of the two behaviours the downstream consumer named, source attribution and the model registry, leaving the extraction with little reason to exist beyond the scanner.

## References

- [CLO-594](https://linear.app/cloud-ai/issue/CLO-594) — Lock the gcm library boundary (this task)
- [Discovery Report](../discovery/clo-594.md) — Problem framing, code exploration, approach analysis
- [ADR-001](001-foundational-architecture-decisions.md) — Foundational architecture decisions (precedent for single-ADR locking)
- [CLO-593](https://linear.app/cloud-ai/issue/CLO-593) — Extract lok's Backend abstraction (predecessor)
- [CLO-595](https://linear.app/cloud-ai/issue/CLO-595) — Ship gcm's secret scanner as a library API (first consumer)
