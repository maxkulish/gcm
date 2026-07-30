# Plan: CLO-598 Verify the gcm library from an out-of-tree consumer and lock its public surface

## Context
- Design: docs/designs/clo-598-consumer-check.md
- Discovery: docs/discovery/clo-598.md
- Linear: https://linear.app/cloud-ai/issue/CLO-598/verify-the-gcm-library-from-an-out-of-tree-consumer-and-lock-its
- Branch: feat/clo-598-consumer-check

## Sub-tasks

### ST1 Create smoke crate skeleton
**Files:** `smoke/Cargo.toml`
**Acceptance:** `cd smoke && cargo build` compiles (empty main.rs)
**Estimate:** S

Create `smoke/Cargo.toml` per the design spec:
- `name = "gcm-smoke"`, `publish = false`
- `gcm = { path = "..", default-features = false }`
- Feature `gcm-cli = ["gcm/cli"]`
- Create `smoke/src/main.rs` with `fn main() {}` stub

### ST2 Write smoke consumer exercising all library APIs
**Files:** `smoke/src/main.rs`
**Acceptance:** `cd smoke && cargo test` passes (5 tests exercising privacy, provider, status, config, paths)
**Estimate:** M

Implement 5 test functions in `smoke/src/main.rs`:
1. `privacy_scanner_compiles_and_scans` — uses `gcm::privacy::{detect, rules, Scanner, SecretScanMode}` to compile a vendored rule pack, scan text, and redact secrets
2. `model_resolution_uses_injected_env` — uses `gcm::provider::resolve_model_with_source` with a closure-based env lookup
3. `model_fetch_degrades_to_fallback` — uses `gcm::provider::models::fetch_supported_models_with` with a failing fetcher, verifies fallback list
4. `status_report_resolves_without_config_file` — uses `gcm::status::build_report` with a caller-supplied env map
5. `config_types_constructible` — constructs a `Config` with `ProviderConfig` and verifies fields

### ST3 Create surface check script
**Files:** `scripts/check-public-surface.sh`
**Acceptance:** `scripts/check-public-surface.sh` exits 0 (no forbidden HTML files found)
**Estimate:** S

Create the script per the design spec:
- Resolve repo root via `git rev-parse --show-toplevel`
- Run `CARGO_TARGET_DIR=target/surface_check cargo doc --lib --no-default-features`
- Assert these files do NOT exist in `target/surface_check/doc/gcm/`:
  - `trait.Provider.html`
  - `struct.ConflictHunk.html`, `struct.ResolveContext.html`, `struct.Resolution.html`
  - `struct.HunkResolution.html`, `struct.ResolveReport.html`, `struct.RoundReport.html`, `struct.FinishReport.html`

### ST4 Add publish=false to root Cargo.toml
**Files:** `Cargo.toml`
**Acceptance:** `cargo metadata --no-deps --format-version 1 | jq '.packages[0].publish'` returns `[]` (publish = false)
**Estimate:** S

Add `publish = false` to the `[package]` section of the root `Cargo.toml` to mechanically enforce ADR-002 Decision 7.

### ST5 Feature combination verification
**Files:** (none new — exercises existing smoke crate)
**Acceptance:**
- `cd smoke && cargo test` passes (library-only)
- `cd smoke && cargo test --features gcm-cli` passes (with CLI features propagated)
- `cargo test --no-default-features --lib` passes (existing library tests)
- `cargo test` passes (all 524 existing tests)
**Estimate:** S

Run the full feature matrix to confirm no regressions.

### ST6 Full pre-merge gate
**Files:** (none new)
**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` passes
**Estimate:** S

Run the full pre-merge gate on the gcm crate to confirm all checks pass with the new `smoke/` and `scripts/` directories present.

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (fmt + clippy + test)

## Risks
- **Smoke crate may fail to compile if any public API type is not itself public** — this is the exact failure mode the task is designed to catch. If it happens, the fix is to make the missing type public (or remove it from the public API). This is a discovery, not a blocker.
- **Surface check script may produce false positives if rustdoc HTML naming conventions differ** — the script uses well-known conventions (`trait.X.html`, `struct.X.html`). If these change, the script will need updating. Low risk.
- **`publish = false` on root Cargo.toml may affect existing publish workflows** — ADR-002 Decision 7 already defers publish, so this is a mechanical enforcement of an existing decision. No impact.
