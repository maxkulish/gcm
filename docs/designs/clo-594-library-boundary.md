# CLO-594: Lock the gcm library boundary, the sync/async seam and the config shape (ADR)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-594
**Status**: Design
**Author**: Team
**Created**: 2026-07-27
**ADR**: [docs/adrs/002-library-boundary.md](../adrs/002-library-boundary.md) (the deliverable; this design doc is the process artifact)

---

## Problem

gcm is an 18,830-line Rust CLI with a single `[[bin]]` target and no `src/lib.rs`. Every module — config, provider, privacy, plan, diff, resolve — is internal to the binary. Two downstream consumers need a library boundary: **lok** (async, tokio+reqwest) needs gcm's secret scanner and config types, and a **remem-ai** memory pipeline fork needs the same. Today there is no library boundary at all: importing any gcm type drags in `cliclack` (the interactive wizard), `clap` (CLI argument parsing), and `ureq` (sync HTTP). Three interdependent decisions — crate shape, sync vs async, config seam — must be locked before any extraction code moves. CLO-593 (lok's Backend abstraction) is the immediate predecessor; CLO-595 (ship the secret scanner as a library API) is the first consumer. (Cited from [discovery report](../discovery/clo-594.md).)

---

## Goals / Non-goals

**Goals:**
- Lock the crate shape: `[lib]` target inside the existing package (not a workspace).
- Lock the sync/async seam: gcm stays sync; only non-transport types cross the boundary.
- Lock the config seam: data types + pure functions in the library; wizard in the binary.
- Answer the `clap` question once for both `SecretScanMode` and `ProviderId`.
- Confirm the `Provider` trait and the wizard stay in the binary, with conditions for revisiting.
- Decide registry: path-only for now (no crates.io publish).

**Non-goals:**
- No code moves under this issue (ADR only).
- No workspace split (too disruptive for a daily-use tool).
- No async rewrite of gcm's transport.
- No `Provider` trait export (rejected per issue scope).

---

## Architecture

### Module layout after extraction

```
gcm (existing package, one Cargo.toml)
├── [lib] target (gcm-core)
│   ├── config: Config, ProviderConfig, ConflictConfig, AutoPolicy
│   │          load, save, apply_to_env, config_path, needs_onboarding
│   ├── provider: ProviderId, AuthMethod
│   └── privacy: SecretScanMode
│
└── [[bin]] target (gcm)
    ├── config: run_wizard, run_provider_wizard, non_tty_instructions
    ├── provider: Provider trait + backends (Groq, Google, OpenAI, Anthropic, Ollama, Vertex)
    ├── cache, cli, debug, diff, git, output, paths, plan, resolve, status, ui
    └── main.rs
```

### Data flow

The library exports pure types and functions. The binary imports them via `use gcm::config::Config` (not `mod config;` — see Assumptions §A1). Consumers (lok, remem-ai) depend on the library via path dependency. No transport, no async, no `cliclack`/`console` in the library build.

### Cargo.toml changes (implementation phase)

```toml
[lib]
name = "gcm"
path = "src/lib.rs"

[features]
default = ["cli"]
cli = ["dep:clap", "dep:cliclack", "dep:console"]
clap = ["dep:clap"]

[dependencies]
clap = { version = "4", features = ["derive"], optional = true }
cliclack = { version = "0.5", default-features = false, optional = true }
console = { version = "0.16", default-features = false, features = ["std"], optional = true }
# ... non-optional deps unchanged
```

---

## Public API surface

```rust
// src/lib.rs — the library re-export surface

pub mod config {
    pub use crate::config::{Config, ProviderConfig, ConflictConfig, AutoPolicy};
    pub use crate::config::{load, save, apply_to_env, config_path, needs_onboarding};
}

pub mod provider {
    pub use crate::provider::{ProviderId, AuthMethod};
}

pub mod privacy {
    pub use crate::privacy::SecretScanMode;
}
```

The binary continues to declare `mod config;` etc. in `main.rs`, but the `[lib]` target in `lib.rs` re-exports the public types. The binary imports from the library crate (`use gcm::config::Config`) to ensure type identity (see Assumptions §A1).

Type signatures for the exported types (unchanged from current code):

```rust
pub struct Config {
    pub version: u32,
    pub default: ProviderId,
    pub providers: Vec<ProviderConfig>,
    pub conflict: ConflictConfig,
}

pub enum ProviderId { Groq, Google, Openai, Anthropic, Ollama, Vertex }

pub enum SecretScanMode { Off, Redact, Abort }

// With optional clap feature:
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderId { ... }
```

---

## Assumptions

| ID | Assumption | Confidence | Verification |
|---|---|---|---|
| A1 | In a single-package `[lib]+[[bin]]` setup, the binary must import shared types from the library crate (`use gcm::config::Config`), not declare `mod config;` in both targets — otherwise the compiler treats them as distinct, incompatible types. | High | Gemini design review flagged this (review §2 Concerns); implementation in CLO-595 will verify via `cargo check` |
| A2 | `cliclack` and `console` must be `optional = true` in `Cargo.toml` and gated behind a `cli` feature — otherwise every library consumer inherits them as non-optional dependencies. | High | Gemini design review flagged this (review §6 Blind Spots); CLO-595 implementation will verify with `cargo tree --no-default-features` |
| A3 | Binary-internal modules (`cache`, `git`, `resolve`, `ui`) do not need `#[cfg(not(feature = "library"))]` — omitting them from `lib.rs` is sufficient to exclude them from the library build. | High | Gemini design review flagged this (review §5); verified by Rust module system semantics |
| A4 | The `save` function's `0600` permission logic must use `#[cfg(unix)]` conditional compilation to avoid breaking Windows builds. | Medium | Gemini design review flagged this (review §4); implementation in CLO-595 will add the `cfg` gate |
| A5 | `GcmError` contains binary-specific variants (`Git`, `Provider`, `OnboardingRequired`); the library may need a lightweight error type or the binary-specific variants must be feature-gated. | Medium | Gemini design review flagged this (review §5); deferred to CLO-595 implementation — the first extraction only exports `SecretScanMode` (no `GcmError` return) |
| A6 | The secret scanner's `SecretScanMode` type is the primary export target for CLO-595; the scanner's `check`/`filter`/`redact` functions stay in the binary for now. | High | CLO-595 scope confirms this; `.pi/lessons/clo-531-resolve-lessons.md § L1` established that secret scan must apply to all egress paths — the library exports the mode, the binary keeps the implementation |
| A7 | Path-only consumption (no crates.io publish) is sufficient because both consumers (lok, remem-ai) are in the same organization. | High | CLO-593 and CLO-595 both use path/git dependencies; revisit when an external consumer requests a publish |

---

## Test plan

Since no code moves under this issue, the test plan is for the ADR itself, not for code:

**Unit / integration tests:** N/A — no code changes. The ADR is validated by its acceptance criteria (see ADR-002 § "Consequences").

**Manual verification:**
1. ADR-002 exists at `docs/adrs/002-library-boundary.md` with Status: Proposed.
2. ADR-002 names, for each decision, the option chosen and the option rejected, with the reason.
3. A reader can tell from the ADR alone where `Config`, `ProviderId`, and `SecretScanMode` live after the extraction.
4. The sync/async decision is stated against lok's async `Backend` as settled in CLO-593, not in the abstract.
5. No code moves under this issue (`git diff main..feat/clo-594-lock -- '*.rs'` is empty; only docs change).

**Edge cases to cover in the implementation (CLO-595+):**
- `cargo build --no-default-features` (library build without `cli` feature) succeeds.
- `cargo build` (binary build with default features) succeeds.
- `cargo tree --no-default-features` does not list `clap`, `cliclack`, or `console`.
- Type identity: `gcm::config::Config` in the binary and `gcm::config::Config` in the library are the same type.

---

## Migration / rollout

1. ADR-002 is committed to `docs/adrs/002-library-boundary.md` with Status: Proposed.
2. The ADR README index is updated.
3. After human review and approval, Status changes to Accepted.
4. The first implementation task (CLO-595) references ADR-002 for the boundary decisions.
5. No migration of existing code — the ADR is the design; CLO-595+ does the extraction.

**Rollback:** The ADR can be superseded by a later ADR if the decisions prove wrong. No code rollback needed because no code moves.

---

## Open questions

All resolved in ADR-002. No open questions remain.