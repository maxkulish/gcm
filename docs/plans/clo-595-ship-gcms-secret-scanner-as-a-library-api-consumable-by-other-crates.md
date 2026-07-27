# Plan: CLO-595 — Ship gcm's secret scanner as a library API consumable by other crates

## Context
- Design: `docs/designs/clo-595-ship-gcms-secret-scanner-as-a-library-api-consumable-by-other-crates.md`
- Discovery: `docs/discovery/clo-595.md`
- PRD: `docs/prds/clo-595-ship-gcms-secret-scanner-as-a-library-api-consumable-by-other-crates.md`
- ADR: `docs/adrs/002-library-boundary.md`
- Linear: https://linear.app/cloud-ai/issue/CLO-595/ship-gcms-secret-scanner-as-a-library-api-consumable-by-other-crates
- Branch: `feat/clo-595-ship-gcms-secret-scanner-as-a-library-api-consumable-by`

## Sub-tasks

### ST1 Add library crate target and feature boundaries
**Files:** `Cargo.toml`, `src/lib.rs`
**Acceptance:** `cargo build --no-default-features` (library-only build succeeds, no bin target)
**Estimate:** S

### ST2 Split privacy into library API + binary facade
**Files:** `src/privacy/mod.rs`, `src/privacy/facade.rs` (new), `src/main.rs`
**Acceptance:** `cargo test privacy --lib`
**Estimate:** M

### ST3 Implement scanner surface API in library privacy module
**Files:** `src/privacy/mod.rs`
**Acceptance:** `cargo test privacy --lib`
**Estimate:** M

### ST4 Update binary-only callsites to consume shared privacy API
**Files:** `src/cli.rs`, `src/main.rs`, `src/resolve/mod.rs`, `src/error.rs`
**Acceptance:** `cargo test --lib`
**Estimate:** M

### ST5 Add external-consumer validation tests and public API docs
**Files:** `src/privacy/mod.rs`, `src/privacy/facade.rs`, `src/error.rs`, `src/lib.rs`, `tests/library_api.rs`
**Acceptance:** `cargo test --test library_api`
**Estimate:** M

### ST6 Verify feature gating and pre-merge matrix in implementation
**Files:** `Cargo.toml`
**Acceptance:**
```bash
cargo tree --no-default-features -e normal | grep -vE "clap|cliclack|console|ureq"
```
and
```bash
cargo build --no-default-features && cargo build && cargo test --no-default-features --test library_api
```
**Estimate:** S

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## Risks
- **Risk:** If `[lib]` + `required-features` conflicts with existing crate metadata assumptions, binary-only consumers may need `--features cli`. **Mitigation:** `required-features = ["cli"]` is validated in ST6 before merge with full build matrix.
- **Risk:** Changing privacy internals via `src/privacy/mod.rs` path-split and `#[path]` usage may regress existing wildcard matching behavior. **Mitigation:** keep `normalize_path`/`wildcard_match` tests in the moved facade tests and add regression coverage for `.gcmignore` filtering in ST2.
- **Risk:** Exporting `CompiledRule` and current rule-collection types may lock `regex` into public ABI; downstream changes to `regex` could become semver-sensitive. **Mitigation:** if needed, follow up CLO-59x to add an accessor layer before `0.6.0` stabilizes.
- **Risk:** Resolving `SecretScanMode` with a generic env lookup might diverge error wording from CLI paths. **Mitigation:** assert exact display strings in unit tests and include `From<ScanError> for GcmError` mapping test in ST4.
