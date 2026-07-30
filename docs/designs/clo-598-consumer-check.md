# Design: CLO-598 - Verify the gcm library from an out-of-tree consumer and lock its public surface

## Problem

The gcm library surface was extracted across four slices (CLO-594 ADR, CLO-595 privacy scanner, CLO-596 provider identity, CLO-597 status resolution), but it has never been exercised from an actual out-of-tree consumer. In-package integration tests (`tests/library_api.rs`, `tests/library_provider_api.rs`, `tests/library_status_api.rs`) compile inside the package: they see gcm's dev-dependencies, its default feature selection, and its module privacy. Three failure modes they cannot catch: (1) a public function whose signature names a type that is not itself public, (2) a path that compiles only because the binary happens to enable a feature, and (3) an accidental export of commit-domain types. The `remem-ai` fork is waiting on a verified, locked surface before it can depend on gcm as a library.

## Goals / Non-goals

**Goals:**
- Create an out-of-tree consumer crate (`smoke/`) that depends on gcm by path and exercises the scanner, model resolution, and status report APIs
- Verify the consumer crate compiles and tests pass with `--no-default-features` (library-only, no CLI deps)
- Verify the consumer crate compiles and tests pass with default features
- Add a CI-checkable surface enumeration script that mechanically verifies no commit-domain types (`Provider` trait, `ConflictHunk`, `ResolveContext`, `Resolution`, `HunkResolution`, `ResolveReport`) appear in the public API
- Verify the publish decision (ADR-002 Decision 7: path-only, no crates.io publish) is documented and correct
- All 524 existing tests still pass

**Non-goals:**
- Publishing gcm to crates.io (ADR-002 Decision 7 defers this until at least one consumer exists in production)
- Changing the library surface (no new public types, no removals, no renames)
- Adding new features or feature gates
- Modifying the binary target
- Creating a workspace or separate crate

## Architecture

### Consumer crate: `smoke/`

A minimal Cargo project at repository root that depends on gcm via path dependency. It is NOT part of the gcm workspace (there is no workspace); it is a standalone crate that happens to live in the same repo for CI convenience.

```
smoke/
├── Cargo.toml          # name = "gcm-smoke", depends on gcm by path
└── src/
    └── main.rs         # exercises scanner, model resolution, status report
```

The consumer crate uses `gcm = { path = "..", default-features = false }` per CLO-594 Lesson L3 (package name is `gcm`, not `gcm-core`).

The smoke crate defines a `gcm-cli` feature that propagates `gcm/cli`, enabling both feature states to be tested:

```toml
[package]
name = "gcm-smoke"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
gcm = { path = "..", default-features = false }

[features]
default = []
gcm-cli = ["gcm/cli"]
```

Both `publish = false` on the smoke crate and `publish = false` on the root `Cargo.toml` mechanically enforce ADR-002 Decision 7 (path-only, no crates.io publish).

### Surface enumeration script: `scripts/check-public-surface.sh`

A bash script that:
1. Resolves the repo root via `git rev-parse --show-toplevel` (portability)
2. Runs `CARGO_TARGET_DIR=target/surface_check cargo doc --lib --no-default-features` to generate docs in an isolated directory (avoids stale artifacts from default-feature builds causing false positives)
3. Checks for the *existence* of specific rustdoc HTML files — since `cargo doc` only generates files for publicly exported items, file existence is a binary, foolproof indicator of a leak (avoids false positives from grep substring matching, e.g., `Provider` matching `ProviderId`)
4. Exits non-zero if any forbidden HTML file exists

The script checks for these forbidden files in `target/surface_check/doc/gcm/`:
- `trait.Provider.html` (the Provider trait — must be binary-only, in `provider/facade.rs`)
- `struct.ConflictHunk.html`, `struct.ResolveContext.html`, `struct.Resolution.html` (commit-domain types)
- `struct.HunkResolution.html`, `struct.ResolveReport.html`, `struct.RoundReport.html`, `struct.FinishReport.html` (resolve module types)

### Data flow

```
┌─────────────┐     path dep      ┌──────────────┐
│  smoke/     │ ───────────────▶  │  gcm (lib)   │
│  main.rs    │   no CLI features │  src/lib.rs  │
│             │                   │              │
│  exercise:  │                   │  config      │
│  - privacy  │                   │  paths       │
│  - provider │                   │  privacy     │
│  - status   │                   │  provider    │
│  - config   │                   │  status      │
└─────────────┘                   │  debug       │
                                  │  (doc hidden)│
                                  └──────────────┘
```

### CI integration

The smoke crate and surface check are run in CI via:
```bash
# Smoke consumer compiles and runs with library-only features
cd smoke && cargo test

# Smoke consumer also compiles with CLI features propagated
cd smoke && cargo test --features gcm-cli

# Surface check: no commit-domain types in public API
scripts/check-public-surface.sh
```

These are additive — no existing CI steps change.

## Public API surface

No changes to the public API. The design verifies the existing surface. The consumer crate exercises these existing public APIs:

```rust
// Privacy scanner (CLO-595)
use gcm::privacy::{detect, rules, ScanError, Scanner, SecretScanMode};

// Provider identity + model registry (CLO-596)
use gcm::provider::{
    models::{fetch_supported_models_with, FetchSource, ModelFetchOutcome},
    resolve_model_with_source, AuthMethod, ErrorKind, ModelSource, ProviderError, ProviderId,
};

// Status report (CLO-597)
use gcm::status::{build_report, PathsStatus, ProviderStatus, StatusReport};

// Config types (CLO-597)
use gcm::config::{Config, ProviderConfig, ConflictConfig, AutoPolicy};

// Path resolution
use gcm::paths::xdg_gcm_dir_from;
```

The consumer crate also verifies that these types are **NOT** accessible:

```rust
// These must NOT compile from an external consumer:
// use gcm::provider::Provider;        // trait — binary-only facade
// use gcm::resolve::ResolveReport;    // module — not in lib.rs
// use gcm::resolve::HunkResolution;   // module — not in lib.rs
```

## Assumptions

- **The `smoke/` crate can depend on gcm by path with `default-features = false`** — confidence: high — verification: the smoke crate's `cargo build --no-default-features` will confirm (CLO-594 Lesson L3: package name is `gcm`, not `gcm-core`)
- **`cargo doc --lib --no-default-features` accurately reflects the public surface** — confidence: high — verification: the surface check script will run this and grep for forbidden types
- **The `Provider` trait is only in `provider/facade.rs`, which is not included in `lib.rs`** — confidence: high — verification: confirmed by `grep "mod facade" src/lib.rs` returning no results; surface check will verify mechanically
- **The entire `resolve/` module is only included in `main.rs`, not in `lib.rs`** — confidence: high — verification: confirmed by `grep "mod resolve" src/lib.rs` returning no results; surface check will verify mechanically
- **`cargo doc --lib` generates HTML files named after type kinds (`trait.X.html`, `struct.X.html`), and file existence is a binary indicator of public export** — confidence: high — verification: the surface check script will confirm by running and checking for specific file paths
- **The 524 existing tests continue to pass after adding `smoke/` and `scripts/`** — confidence: high — verification: `cargo test` in the gcm crate after adding the new directories

## Test plan

### Smoke consumer tests (`smoke/src/main.rs`)

The smoke crate exercises each library API surface with a minimal integration test:

1. **`smoke::privacy_scanner_compiles_and_scans`** — uses `gcm::privacy::{detect, rules, Scanner, SecretScanMode}` to compile a vendored rule pack, scan text, and redact secrets
2. **`smoke::model_resolution_uses_injected_env`** — uses `gcm::provider::resolve_model_with_source` with a closure-based env lookup
3. **`smoke::model_fetch_degrades_to_fallback`** — uses `gcm::provider::models::fetch_supported_models_with` with a failing fetcher, verifies fallback list
4. **`smoke::status_report_resolves_without_config_file`** — uses `gcm::status::build_report` with a caller-supplied env map
5. **`smoke::config_types_constructible`** — constructs a `Config` with `ProviderConfig` and verifies fields

### Surface check (`scripts/check-public-surface.sh`)

1. Resolves repo root via `git rev-parse --show-toplevel`
2. Runs `CARGO_TARGET_DIR=target/surface_check cargo doc --lib --no-default-features`
3. Checks for existence of forbidden HTML files: `trait.Provider.html`, `struct.ConflictHunk.html`, `struct.ResolveContext.html`, `struct.Resolution.html`, `struct.HunkResolution.html`, `struct.ResolveReport.html`, `struct.RoundReport.html`, `struct.FinishReport.html`
4. Exits 0 if none found, exits 1 if any exist in `target/surface_check/doc/gcm/`

### Feature combination verification

1. `cd smoke && cargo test` — library-only (smoke defaults to `default-features = false` on gcm)
2. `cd smoke && cargo test --features gcm-cli` — with CLI features propagated to gcm
3. `cargo test --no-default-features --lib` — existing library tests still pass
4. `cargo test` — all 524 existing tests still pass

### Manual verification

1. `cargo doc --lib --no-default-features --open` — visual inspection of public surface
2. Confirm no commit-domain types appear in the rendered docs

## Migration / rollout

This change is **purely additive**. No existing code is modified. New directories `smoke/` and `scripts/` are added at repository root. No feature flags, no backward compatibility concerns, no rollout order dependencies.

The `smoke/` crate is not published and is not part of any workspace. It exists solely to verify the library surface from an external perspective. It should not be included in `cargo publish` (the gcm `Cargo.toml` excludes it naturally since it's a separate crate).

### Publish decision

ADR-002 Decision 7 (path-only, no publish until at least one consumer exists) is confirmed as the correct decision. The `remem-ai` fork will be the first consumer; until it depends on gcm in production, the path-only approach remains. No changes to this decision are needed.

## Open questions

None. All decisions were locked in ADR-002 and the preceding extraction slices (CLO-594/595/596/597). This task verifies the existing surface; it does not change it.