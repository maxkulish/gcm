# CLO-597 Implementation Plan: Expose source-attributed status resolution through the gcm library

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-597
**Design Document**: [docs/designs/clo-597-expose-status-resolution.md](../designs/clo-597-expose-status-resolution.md)
**Created**: 2026-07-30
**Overall Progress**: 100% (60/60 tasks completed)

---

## Architecture Context

CLO-597 extends the library boundary work from CLO-595/596 by moving the `status` and `config` modules from the binary into the `gcm` library. The binary keeps only thin facades that re-export library types and host the binary-specific entry points (`run_status_subcommand` and the `cliclack` wizard). `SCHEMA_VERSION` and `VERSION` stay binary-side and are passed as parameters to the library's `build_report` function, preserving byte-identical `gcm status` output.

---

## Tasks

### Phase 1: Move Ollama helpers to `src/provider/identity.rs`

- [x] Move `DEFAULT_BASE_URL` constant from `src/provider/ollama.rs` to `src/provider/identity.rs` and make it `pub`
- [x] Move `DEFAULT_PORT` constant from `src/provider/ollama.rs` to `src/provider/identity.rs` and make it `pub`
- [x] Move `normalize_host` function to `src/provider/identity.rs` and make it `pub`
- [x] Move `has_port` private helper to `src/provider/identity.rs`
- [x] Move `is_cloud_model` function to `src/provider/identity.rs` and make it `pub`
- [x] Relocate `normalize_host_variants` unit test from `src/provider/ollama.rs` to `src/provider/identity.rs`
- [x] Relocate `is_cloud_model_detects_both_suffixes` unit test from `src/provider/ollama.rs` to `src/provider/identity.rs`
- [x] Update `src/provider/ollama.rs` to import `normalize_host`, `is_cloud_model`, `DEFAULT_BASE_URL`, `DEFAULT_PORT` from `gcm::provider::identity`
- [x] Verify `cargo test` passes after Phase 1

### Phase 2: Split `config.rs` into library `config/mod.rs` + binary `config/facade.rs`

- [x] Create `src/config/` directory
- [x] Move `src/config.rs` to `src/config/mod.rs`
- [x] In `src/config/mod.rs`, remove `use crate::error::GcmError` and all `cliclack`/`console`/`Command`/`Stdio` imports
- [x] Keep pure types and functions in `src/config/mod.rs`: `Config`, `ProviderConfig`, `ConflictConfig`, `AutoPolicy`, `load`, `save`, `config_path`, `apply_to_env`, `needs_onboarding`
- [x] Keep pure wizard helpers in `src/config/mod.rs`: `wizard_model_list`, `wizard_model_hint`, `wizard_persist_key`, `canonicalize_model`, plus their unit tests
- [x] Create `src/config/facade.rs` starting with `pub use gcm::config::*;`
- [x] Move binary-only wizard functions to `src/config/facade.rs`: `run_wizard`, `run_provider_wizard`, `non_tty_instructions`, `prompt_vertex_target`, `prompt_ollama_endpoint`, `wizard_cancelled`, `wizard_io`, `wizard_read_line`, `read_line`, `any_cloud_key_set`, `cloud_providers`, `env_plan`, `should_onboard`
- [x] In `src/config/facade.rs`, add `use crate::error::GcmError;` and `cliclack`/`console`/`Command`/`Stdio` imports
- [x] Update `src/main.rs`: change `mod config;` to `#[path = "config/facade.rs"] mod config;`
- [x] Ensure binary references (`src/cli.rs`, `src/resolve/mod.rs`) continue to resolve `crate::config::AutoPolicy` via the facade re-export
- [x] Verify `cargo test` passes after Phase 2

### Phase 3: Split `status.rs` into library `status/mod.rs` + binary `status/facade.rs`

- [x] Create `src/status/` directory
- [x] Move `src/status.rs` to `src/status/mod.rs`
- [x] In `src/status/mod.rs`, replace `use crate::config::{self, Config};` with `use gcm::config::{self, Config};`
- [x] In `src/status/mod.rs`, replace `use crate::provider::{...}` with `use gcm::provider::{resolve_model_with_source, AuthMethod, ModelSource, ProviderId};` and `use gcm::provider::identity::{DEFAULT_BASE_URL, normalize_host, is_cloud_model};`
- [x] In `src/status/mod.rs`, remove `use crate::cli::Cli;` and `use crate::output::SCHEMA_VERSION;` and `use crate::cli::VERSION`
- [x] Update `build_report` signature to accept `schema_version: i32` and `version: &'static str` parameters
- [x] Make `StatusReport`, `PathsStatus`, `ProviderStatus`, `build_report`, `print_human`, and `PROVIDER_ORDER` public in `src/status/mod.rs`
- [x] Create `src/status/facade.rs` starting with `pub use gcm::status::*;`
- [x] Move `run_status_subcommand(args: &Cli) -> i32` to `src/status/facade.rs`
- [x] In `src/status/facade.rs`, call `gcm::status::build_report` with `SCHEMA_VERSION` and `crate::cli::VERSION` as the last two arguments
- [x] Update `src/main.rs`: change `mod status;` to `#[path = "status/facade.rs"] mod status;`
- [x] Verify `cargo test` passes after Phase 3

### Phase 4: Library consumer test + no-default-features verification

- [x] Add `pub mod config;` and `pub mod status;` to `src/lib.rs`
- [x] Create `tests/library_status_api.rs`
- [x] In `tests/library_status_api.rs`, construct a `gcm::config::Config` programmatically
- [x] Call `gcm::status::build_report` with an injected env map closure, `schema_version: 1`, and `version: "test"`
- [x] Assert on `StatusReport` fields (selected provider, model source, key source, etc.)
- [x] Ensure `tests/library_status_api.rs` has no `cli` feature dependency
- [x] Verify `cargo test --no-default-features --lib` compiles
- [x] Verify `cargo test --test library_status_api` passes
- [x] Verify `cargo test --test library_provider_api` still passes (CLO-596 regression)

### Phase 5: Testing & Validation

- [x] Run full test suite: `cargo test`
- [x] Confirm 522+ tests pass with 0 failures
- [x] Run `cargo clippy` and ensure clean output
- [x] Verify `cargo test --no-default-features --lib` compiles
- [x] Verify byte-identical `gcm status` output:
  - [x] Check out baseline binary or use `git stash` method
  - [x] Run `gcm status` before changes, save to `/tmp/status-old.txt`
  - [x] Run `gcm status` after changes, save to `/tmp/status-new.txt`
  - [x] Run `gcm status --json` before changes, save to `/tmp/status-json-old.txt`
  - [x] Run `gcm status --json` after changes, save to `/tmp/status-json-new.txt`
  - [x] `diff` old vs new for both human and JSON outputs; expect no differences
- [x] Verify `cargo fmt --check` passes (or run `cargo fmt`)

### Phase 6: Finalization

- [x] Stage all changes: `git add -A`
- [x] Commit with conventional message: `feat(CLO-597): expose source-attributed status resolution through gcm library`
- [x] Push branch: `git push origin feat/clo-597-status-res`
- [x] Create PR: `gh pr create --title "feat(CLO-597): expose source-attributed status resolution through gcm library" --body "..."`
- [x] Link PR to Linear task CLO-597
- [x] Post PR link to Linear comment
- [x] Request review

---

## Module Structure

- `src/lib.rs` - Add `pub mod config;` and `pub mod status;`
- `src/config/mod.rs` - Library module: config types and pure functions
- `src/config/facade.rs` - Binary facade: re-exports + wizard functions
- `src/status/mod.rs` - Library module: status structs, `build_report`, helpers, `print_human`
- `src/status/facade.rs` - Binary facade: re-exports + `run_status_subcommand`
- `src/provider/identity.rs` - Library module: add `is_cloud_model`, `normalize_host`, `DEFAULT_BASE_URL`, `DEFAULT_PORT`
- `src/provider/ollama.rs` - Binary backend: import helpers from library
- `src/main.rs` - Update `mod config;` and `mod status;` to use facade paths
- `tests/library_status_api.rs` - New library consumer integration test

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. The overall percentage will be recalculated based on completed tasks.

---

## Notes

- Keep each phase independently testable.
- Do not change `StatusReport` struct shape — byte-identical output is required.
- Do not move `GcmError` to the library; keep wizard functions in the binary facade.
- Follow the CLO-596 facade pattern exactly (`#[path = ".../facade.rs"] mod ...` + `pub use gcm::*`).
- Attribution helpers must remain env-pure (take `env_lookup` closure, not process env).