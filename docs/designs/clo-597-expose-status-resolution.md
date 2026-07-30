# Design: CLO-597 — Expose source-attributed status resolution through the gcm library

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-597
**Status**: Design
**Created**: 2026-07-30
**Discovery**: [docs/discovery/clo-597.md](../discovery/clo-597.md)
**PRD**: [docs/prds/clo-597-status-resolution.md](../prds/clo-597-status-resolution.md)
**ADR**: [docs/adrs/002-library-boundary.md](../adrs/002-library-boundary.md)
**Approach**: Approach B — In-place gating for config, facade split for status

---

## Problem

`src/status.rs` (1,010 lines) is binary-only. Its sole entry point `run_status_subcommand(args: &Cli)` takes a `clap` struct, and the module imports `crate::config::{self, Config}` (binary-only), `crate::output::SCHEMA_VERSION` (binary-only), and `crate::cli::VERSION` (binary-only). The attribution helpers are already env-pure (they take an `env_lookup` closure), but four binary-only imports block the library boundary.

CLO-596 already moved `ProviderId`, `AuthMethod`, `ModelSource`, `resolve_model_with_source` into `gcm::provider`. What remains:
1. **`Config` types** — still binary-only (`mod config;` in `main.rs`), ADR-002 Decision 3 says they cross
2. **`SCHEMA_VERSION` / `VERSION`** — binary-only, must stay (passed as parameters to library)
3. **Three `ollama` pure helpers** — `is_cloud_model`, `normalize_host`, `DEFAULT_BASE_URL` trapped in the binary's ollama backend

## Goals / Non-goals

**Goals:**
- Move `Config`, `ProviderConfig`, `ConflictConfig`, `AutoPolicy` and pure functions (`load`, `save`, `apply_to_env`, `config_path`, `needs_onboarding`) to the library
- Gate the `cliclack` wizard functions behind `#[cfg(feature = "cli")]` so the library has no `cliclack`/`console` dependency without the `cli` feature
- Split `status.rs` into a library module (`src/status/mod.rs` — structs + `build_report` + helpers) and a binary facade (`src/status/facade.rs` — `run_status_subcommand`)
- Move `ollama::is_cloud_model`, `normalize_host`, `has_port`, `DEFAULT_BASE_URL`, `DEFAULT_PORT` to `src/provider/identity.rs`
- The library's `build_report` takes `schema_version: i32` and `version: &'static str` as parameters — the binary passes `SCHEMA_VERSION` and `VERSION`
- `cargo test --no-default-features` compiles the library status + config surface
- Maintain type identity: binary's `crate::config::Config` is `gcm::config::Config` (re-exported from library, not re-declared)

**Non-goals:**
- No extraction of `run_status_subcommand` (stays in binary, takes `&Cli`)
- No extraction of `output.rs` (`SCHEMA_VERSION` stays binary-side)
- No change to `gcm status` output format or exit codes
- No crates.io publish (ADR-002 Decision 7)
- No async migration, no transport refactor (ADR-002 Decision 2)

## Architecture

### Crate shape (extended from CLO-595/596)

```
gcm package (one Cargo.toml, two targets)

  [lib] gcm  -> src/lib.rs
    pub mod privacy;                 (CLO-595)
    pub mod provider;                (CLO-596)
    pub mod config;                  src/config.rs          ← NEW: library module
    pub mod status;                  src/status/mod.rs      ← NEW: library module
    #[doc(hidden)] pub mod debug;

  [[bin]] gcm (required-features = ["cli"]) -> src/main.rs
    #[path = "privacy/facade.rs"]   mod privacy;   (CLO-595)
    #[path = "provider/facade.rs"]  mod provider;  (CLO-596)
    #[path = "config/facade.rs"]    mod config;    ← NEW: re-exports + wizard
    #[path = "status/facade.rs"]    mod status;    ← NEW: re-exports + run_status_subcommand
    mod cache, cli, debug, diff, error, git, output, paths, plan, resolve, ui
```

### File-by-file changes

| Path | Change |
|---|---|
| `src/lib.rs` | Add `pub mod config;` and `pub mod status;` |
| `src/config.rs` → `src/config/mod.rs` | **Moved** — library module: types + pure functions; wizard functions removed (moved to facade) |
| `src/config/facade.rs` | **New** — `pub use gcm::config::*;` + wizard functions (`run_wizard`, `run_provider_wizard`, `non_tty_instructions`, `prompt_*`, `wizard_*`) moved from `config.rs`; depends on `cliclack`/`console`/`GcmError` |
| `src/status.rs` → `src/status/mod.rs` | **Moved** — library module: `StatusReport`, `PathsStatus`, `ProviderStatus`, `build_report`, `paths_status`, `selected_provider`, `is_activated`, `key_source`, `ollama_endpoint`, `config_model`, `model_source_label`, `vertex_*`, `env_value`, `env_nonblank`, `print_human`, `locality_tag`, `print_provider_section`, `print_provider_block`, unit tests |
| `src/status/facade.rs` | **New** — `pub use gcm::status::*;` + `run_status_subcommand(args: &Cli) -> i32` (calls `gcm::status::build_report` with `SCHEMA_VERSION` and `VERSION`) |
| `src/provider/identity.rs` | **Modified** — add `is_cloud_model`, `normalize_host`, `has_port`, `DEFAULT_BASE_URL`, `DEFAULT_PORT` (moved from `src/provider/ollama.rs`) |
| `src/provider/ollama.rs` | **Modified** — import `normalize_host`, `is_cloud_model`, `DEFAULT_BASE_URL`, `DEFAULT_PORT` from `gcm::provider::identity` (or via facade re-export); remove the local definitions |
| `src/main.rs` | `mod config;` → `#[path = "config/facade.rs"] mod config;`; `mod status;` → `#[path = "status/facade.rs"] mod status;` |
| `src/cli.rs` | No change — `use crate::config::AutoPolicy` resolves to facade re-export |
| `src/resolve/mod.rs` | No change — `use crate::config::*` resolves to facade re-exports |
| `src/error.rs` | No change — stays binary-only |
| `src/output.rs` | No change — `SCHEMA_VERSION` stays binary-side |
| `Cargo.toml` | No change — feature setup already correct from CLO-594/595 |
| `tests/status.rs` | **No change** — runs the compiled binary as a subprocess (`CARGO_BIN_EXE_gcm`), does not import from `crate::status` or `gcm::status` directly |
| `tests/library_status_api.rs` | **New** — library consumer test: resolves a full status report through the library using a caller-supplied env map, with no gcm config file on disk |

### Why config.rs must split (not just gate)

ADR-002 Decision 3 deferred the config file split, saying `#[cfg(not(feature = "library"))]` gating is "sufficient for v1." But the wizard functions return `GcmError`, which is binary-only (`mod error;` in `main.rs`, not in `lib.rs`). If `config.rs` becomes a library module with the wizard gated behind `#[cfg(feature = "cli")]`, the wizard still can't compile in the library context because `crate::error::GcmError` doesn't exist there — `crate::` in the library refers to `gcm::`, not the binary.

Moving `GcmError` to the library is out of scope (it has binary-specific variants: `Git`, `Provider`, `OnboardingRequired`, `NonInteractive`, etc.). So the wizard functions must live in the binary facade. This requires a file split:
- `src/config/mod.rs` — library: types + pure functions (no `GcmError`, no `cliclack`)
- `src/config/facade.rs` — binary: re-exports + wizard functions (uses `GcmError`, `cliclack`, `console`)

This is the split the ADR anticipated: "A file split can happen when the library surface stabilizes." CLO-597 is the extraction that makes it necessary.

### `build_report` signature change

Current (binary-only):
```rust
fn build_report(
    cli_provider: Option<ProviderId>,
    cli_model: Option<&str>,
    config: Option<&Config>,
    env_lookup: impl Fn(&str) -> Option<String>,
) -> StatusReport {
    // ...
    StatusReport {
        v: SCHEMA_VERSION,           // binary-only constant
        version: crate::cli::VERSION, // binary-only constant
        // ...
    }
}
```

After (library-public):
```rust
pub fn build_report(
    cli_provider: Option<ProviderId>,
    cli_model: Option<&str>,
    config: Option<&Config>,
    env_lookup: impl Fn(&str) -> Option<String>,
    schema_version: i32,
    version: &'static str,
) -> StatusReport {
    // ...
    StatusReport {
        v: schema_version,
        version,
        // ...
    }
}
```

The binary's `run_status_subcommand` passes `SCHEMA_VERSION` and `crate::cli::VERSION`. A library consumer passes their own values. The `StatusReport` struct shape is unchanged — byte-identical output is preserved.

### Ollama helpers in identity.rs

Three pure functions and two constants move from `src/provider/ollama.rs` to `src/provider/identity.rs`:

```rust
// In src/provider/identity.rs (library)

/// Default Ollama endpoint.
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";
/// Default Ollama port (used by `normalize_host` when the host has no port).
pub const DEFAULT_PORT: &str = "11434";

/// Normalize an `OLLAMA_HOST` value into a base URL.
pub fn normalize_host(host: &str) -> String { ... }

/// Whether an Ollama model routes off-machine through Ollama Cloud.
pub fn is_cloud_model(model: &str) -> bool { ... }

/// Whether a scheme-less host string carries an explicit numeric port.
fn has_port(h: &str) -> bool { ... }
```

The binary's `src/provider/ollama.rs` imports these from `gcm::provider::identity` (via the facade re-export). The `resolve_base_url` function in the ollama backend uses `normalize_host`, `DEFAULT_BASE_URL`, and `DEFAULT_PORT` — all now from the library.

### Data flow

```
  external crate (lok / remem-ai)            gcm binary
  -------------------------------            ----------
  gcm::config::Config                       crate::config::Config
          |                                         |
          |                                facade re-exports library
          |                                         |
  gcm::status::build_report                crate::status::run_status_subcommand
    (provider, model, config, env,         (args: &Cli)
     schema_version, version)                       |
          |                                config::load()
          |                                gcm::status::build_report(
          |                                  args.provider, args.model,
          |                                  config, env_lookup,
          |                                  SCHEMA_VERSION, VERSION)
          |                                         |
          v                                         v
  gcm::status::StatusReport                crate::status::StatusReport
          |                                (same type — re-exported)
          v
  gcm::status::print_human(&report)       crate::status::print_human(&report)
    (optional — library consumer             (binary calls it for --human)
     can use their own formatter)
```

### Visibility widening

| Item | Current | After | Reason |
|---|---|---|---|
| `status::StatusReport` | `pub` (binary) | `pub` (library) | Library consumer needs the return type |
| `status::PathsStatus` | `pub` (binary) | `pub` (library) | Field of `StatusReport` |
| `status::ProviderStatus` | `pub` (binary) | `pub` (library) | Field of `StatusReport` |
| `status::build_report` | `fn` (private) | `pub fn` (library) | Library entry point |
| `status::print_human` | `fn` (private) | `pub fn` (library) | Reusable formatter |
| `status::PROVIDER_ORDER` | `const` (private) | `pub(crate) const` (library) | Used by tests |
| `config::Config` | `pub` (binary) | `pub` (library) | Library consumer needs it |
| `config::ProviderConfig` | `pub` (binary) | `pub` (library) | Field of `Config` |
| `config::ConflictConfig` | `pub` (binary) | `pub` (library) | Field of `Config` |
| `config::AutoPolicy` | `pub` (binary) | `pub` (library) | Field of `Config` |
| `config::load` | `pub` (binary) | `pub` (library) | Library consumer |
| `config::save` | `pub` (binary) | `pub` (library) | Library consumer |
| `config::config_path` | `pub` (binary) | `pub` (library) | Used by `status::build_report` |
| `config::apply_to_env` | `pub` (binary) | `pub` (library) | Library consumer |
| `config::needs_onboarding` | `pub` (binary) | `pub` (library) | Library consumer |
| `provider::identity::is_cloud_model` | `pub(crate)` (binary) | `pub` (library) | Used by `status::build_report` |
| `provider::identity::normalize_host` | `pub(crate)` (binary) | `pub` (library) | Used by `status::ollama_endpoint` |
| `provider::identity::DEFAULT_BASE_URL` | `pub(crate)` (binary) | `pub` (library) | Used by `status::ollama_endpoint` |
| `provider::identity::DEFAULT_PORT` | `const` (private) | `pub` (library) | Used by `normalize_host` |

## Implementation Plan

### Phase 1: Move ollama helpers to identity.rs

- [ ] Move `is_cloud_model`, `normalize_host`, `has_port`, `DEFAULT_BASE_URL`, `DEFAULT_PORT` from `src/provider/ollama.rs` to `src/provider/identity.rs`
- [ ] Make them `pub` in `identity.rs`
- [ ] Update `src/provider/ollama.rs` to import from `gcm::provider::identity` (via facade re-exports)
- [ ] Verify `cargo test` passes

### Phase 2: Split config.rs into library + facade

- [ ] Create `src/config/` directory
- [ ] Move `src/config.rs` to `src/config/mod.rs` — library module with types + pure functions only
- [ ] Move wizard functions (`run_wizard`, `run_provider_wizard`, `non_tty_instructions`, `prompt_vertex_target`, `prompt_ollama_endpoint`, `wizard_cancelled`, `wizard_io`, `wizard_read_line`, `read_line`, `any_cloud_key_set`, `cloud_providers`, `env_nonblank`, `env_plan`, `should_onboard`) to `src/config/facade.rs`
- [ ] `src/config/facade.rs` starts with `pub use gcm::config::*;` then has the wizard functions
- [ ] Remove `use crate::error::GcmError` from `config/mod.rs` (not needed — pure functions don't use it)
- [ ] Remove `cliclack`/`console`/`Command`/`Stdio` imports from `config/mod.rs`
- [ ] Update `src/main.rs`: `mod config;` → `#[path = "config/facade.rs"] mod config;`
- [ ] Verify `cargo test` passes

### Phase 3: Split status.rs into library + facade

- [ ] Create `src/status/` directory
- [ ] Move `src/status.rs` to `src/status/mod.rs` — library module with structs + `build_report` + all helpers + `print_human` + unit tests
- [ ] Change `build_report` signature to accept `schema_version: i32` and `version: &'static str`
- [ ] Remove `use crate::cli::Cli`, `use crate::output::SCHEMA_VERSION`, `use crate::config::{self, Config}` — replace with `use gcm::config::{self, Config}`
- [ ] Remove `use crate::provider::{ollama, ...}` — replace with `use gcm::provider::{identity, ...}` (for ollama helpers) and `use gcm::provider::{resolve_model_with_source, AuthMethod, ModelSource, ProviderId}`
- [ ] Make `build_report`, `print_human`, `StatusReport`, `PathsStatus`, `ProviderStatus` `pub`
- [ ] Create `src/status/facade.rs` — `pub use gcm::status::*;` + `run_status_subcommand(args: &Cli) -> i32`
- [ ] `run_status_subcommand` calls `gcm::status::build_report(..., SCHEMA_VERSION, crate::cli::VERSION)`
- [ ] Update `src/main.rs`: `mod status;` → `#[path = "status/facade.rs"] mod status;`
- [ ] `tests/status.rs` needs no changes (runs binary as subprocess, no `crate::status` imports)
- [ ] Move unit tests for ollama helpers (`normalize_host_variants`, `is_cloud_model_detects_both_suffixes`) from `src/provider/ollama.rs` to `src/provider/identity.rs`
- [ ] Place pure wizard helpers (`wizard_model_list`, `wizard_model_hint`, `wizard_persist_key`, `canonicalize_model`) in `src/config/mod.rs` (library) — they don't use `cliclack` or `GcmError`
- [ ] Verify `cargo test` passes

### Phase 4: Library consumer test + no-default-features check

- [ ] Create `tests/library_status_api.rs` — resolves a full status report through the library using a caller-supplied env map, with no gcm config file on disk
- [ ] Verify `cargo test --no-default-features --lib` compiles the library status + config surface
- [ ] Verify `cargo test` (all features) passes with 522+ tests
- [ ] Verify `gcm status` and `gcm status --json` produce byte-identical output to v0.6.0

## Constraints

**Must:**
- `gcm status` and `gcm status --json` produce byte-identical output to v0.6.0 for the same config and environment
- The library's status path pulls in neither `clap` nor `cliclack` (verified by `cargo test --no-default-features`)
- Type identity: `crate::config::Config` (binary) is `gcm::config::Config` (library) — the binary imports from the library, not re-declares
- All 522 existing tests continue to pass
- The `env_lookup` closure pattern survives the move — the library's `build_report` stays env-pure

**Must-not:**
- Must not move `GcmError` to the library (binary-specific variants, out of scope)
- Must not move `SCHEMA_VERSION` or `VERSION` to the library (binary-side concerns)
- Must not change the `StatusReport` struct shape (byte-identical output)
- Must not change `gcm status` exit codes or output format
- Must not add `cliclack` or `console` as non-optional dependencies

**Prefer:**
- Follow the CLO-596 facade pattern exactly (`#[path]` + `pub use gcm::*`)
- Keep the precedence chain legible (don't collapse the attribution helpers)

**Escalate when:**
- If `config.rs` split reveals unexpected `crate::` dependencies in the pure functions (should not happen — verified during discovery)
- If `build_report` signature change breaks any test beyond import-path fixes

## Acceptance Criteria

- [ ] AC1: `cargo test` passes with 522+ tests (0 failures), including `tests/status.rs` (17 tests) covering the same behaviour
- [ ] AC2: `gcm status` and `gcm status --json` produce byte-identical output to v0.6.0 — verified by running both and diffing
- [ ] AC3: `tests/library_status_api.rs` resolves a full status report through `gcm::status::build_report` using a caller-supplied env map, with no gcm config file on disk
- [ ] AC4: `cargo test --no-default-features --lib` compiles — the library's status + config surface has no `clap` or `cliclack` dependency. `tests/library_status_api.rs` must also compile and run under `--no-default-features` (it uses only library types, no `cli` feature)
- [ ] AC5: `cargo test --no-default-features` compiles the library provider surface (regression check — CLO-596's `tests/library_provider_api.rs` still passes)

**Verification method**: `cargo test && cargo test --no-default-features --lib && cargo clippy`

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | All existing tests pass | 522+ passed, 0 failed | `cargo test` |
| 2 | `gcm status` byte-identical to v0.6.0 | Output matches exactly | `gcm status > /tmp/status-new.txt; git stash; gcm status > /tmp/status-old.txt; git stash pop; diff /tmp/status-old.txt /tmp/status-new.txt` |
| 3 | `gcm status --json` byte-identical to v0.6.0 | JSON matches exactly | Same as above with `--json` flag |
| 4 | Library consumer test | StatusReport resolved with injected env, no config file | `cargo test --test library_status_api` |
| 5 | No-default-features library compiles | Compiles without `clap`/`cliclack` | `cargo test --no-default-features --lib` |
| 6 | Provider regression | CLO-596 library test still passes | `cargo test --test library_provider_api` |
| 7 | Clippy clean | No warnings | `cargo clippy` |

**Edge cases to cover:**
- Library consumer with no config file on disk (config = `None`)
- Library consumer with a config file but no env vars set
- `GCM_PROVIDER` set to an unknown value (reported, not fatal)
- Ollama with `OLLAMA_HOST` set to a host without a port (normalize_host adds default port)
- Ollama with a `:cloud` model (zero_egress = false)

## Testing Strategy

- **Unit Tests**: The 15 existing unit tests in `src/status.rs` move to `src/status/mod.rs` unchanged — they already test `build_report` with injected env
- **Integration Tests**: The 17 existing tests in `tests/status.rs` update import paths from `crate::status` to `gcm::status` — otherwise unchanged
- **Library Consumer Test**: New `tests/library_status_api.rs` — constructs a `Config` programmatically, calls `gcm::status::build_report` with an injected env map, asserts the report fields
- **No-default-features**: `cargo test --no-default-features --lib` verifies the library compiles without `clap`/`cliclack`
- **Byte-identical output**: Run `gcm status` and `gcm status --json` before and after the change, diff the output

## Open Questions

- [ ] Should `print_human` be `pub` in the library? (Decision: yes — it's pure and a library consumer may want the same formatted output. If not desired, it can be made `pub(crate)` later without breaking changes.)

## References

- [Linear Task](https://linear.app/cloud-ai/issue/CLO-597)
- [ADR-002: Library Boundary](../adrs/002-library-boundary.md)
- [CLO-596 Design](clo-596-expose-provider-identity-and-registry.md) — predecessor, facade pattern reference
- [Discovery Report](../discovery/clo-597.md)
- [PRD](../prds/clo-597-status-resolution.md)