# Design: CLO-595 - Ship gcm's secret scanner as a library API consumable by other crates

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-595
**Status**: Design
**Created**: 2026-07-27
**Discovery**: [docs/discovery/clo-595.md](../discovery/clo-595.md)
**PRD**: [docs/prds/clo-595-ship-gcms-secret-scanner-as-a-library-api-consumable-by-other-crates.md](../prds/clo-595-ship-gcms-secret-scanner-as-a-library-api-consumable-by-other-crates.md)
**ADR**: [docs/adrs/002-library-boundary.md](../adrs/002-library-boundary.md)
**Approach**: Approach A - in-package `[lib]` target with scanner-focused exports

---

## Problem

Downstream in-org consumers (`lok`, the `remem-ai` memory pipeline) need gcm's secret scanner, but today they cannot import it at any price short of forking it. Discovery confirmed the shape of the blockage: `Cargo.toml` declares a single `[[bin]]` target and no `[lib]`, so every scanner symbol lives behind `crate::`-scoped modules in the binary; `SecretScanMode` at `src/privacy/mod.rs:15` derives `clap::ValueEnum` and resolves itself by reading `std::env::var("GCM_SECRET_SCAN")` directly; and the one path that reports a detection, `Privacy::scan_text` at `src/privacy/mod.rs:88`, returns `GcmError::SecretDetected { count }`, a binary-oriented error that drags `ProviderError` and the whole commit-flow taxonomy behind it. The detection core itself is in good shape (`src/privacy/detect.rs`, `rules.rs`, and `entropy.rs` import nothing outside `regex`, `serde`, `toml`, and each other), which is why discovery scored the baseline 4/10 on coupling rather than on quality: this is API seam work, not a rewrite. It matters now because ADR-002 already locked the boundary decisions this ticket implements, and because every week the consumer waits is a week closer to a duplicated scanner drifting away from gcm's FR-60 rule corpus.

---

## Goals / Non-goals

**Goals:**

- Add a `[lib]` target (`src/lib.rs`) to the existing `gcm` package, per ADR-002 Decision 1. No workspace.
- Export the scanner surface named in ADR-002: `SecretScanMode`, `rules::{RuleEngine, CompiledRule, vendored}`, `detect::{secret_ranges, redact_secrets, merge_ranges}`, `entropy::{Charset, shannon_entropy, normalized_entropy}`.
- Replace `SecretScanMode::resolve`'s direct process-env read with `resolve_with`, which takes a caller-supplied lookup closure (PRD S2).
- Define a library-local `ScanError` for the abort path and map it back to `GcmError` in the binary, so no consumer needs `GcmError` to use abort mode (PRD S3, FR4; ADR-002 Implementation Note 5).
- Make `clap`, `cliclack`, and `console` optional behind a default-on `cli` feature so the library build pulls none of them (PRD FR5, AC5).
- Keep `.gcmignore` filtering, `Repo`/`ChangedFile`/`GatheredDiff` wiring, and the `Privacy` facade in the binary (PRD S4; ADR-002 binary-only list).
- Prove the surface from an external-consumer vantage point with an integration test under `tests/` whose caller-side signatures name only `std` and `gcm::privacy` types (PRD AC2, AC3).

**Non-goals:**

- No workspace split into `gcm-core` (Approach B, rejected in discovery).
- No change to `--secret-scan` CLI semantics, defaults, exit codes, or user-facing messages. Redact and abort must behave identically to v0.5.2 on the same input (PRD AC4).
- No extraction of `config`, `provider`, or `status` into the library. Those are CLO-596 and CLO-597; `src/lib.rs` starts with `privacy` only.
- No `Provider` trait export, no async migration, no transport refactor (ADR-002 Decisions 2 and 5).
- No crates.io publish. Path dependency only (ADR-002 Decision 7).
- No new rule syntax, no `--secret-rules` file loading, no change to the vendored corpus in `src/privacy/rules.toml`.

---

## Architecture

The package keeps one `Cargo.toml` and gains a second compilation target. The split runs straight through `src/privacy/`: the detection core and the mode enum become library modules, and the git-shaped facade that wraps them becomes a binary-only file that happens to sit in the same directory.

```
gcm package (one Cargo.toml, two targets)

  [lib] gcm  -> src/lib.rs
    pub mod privacy;              src/privacy/mod.rs
      SecretScanMode              (clap::ValueEnum behind #[cfg_attr])
      ScanError                   NEW - library-local error
      Scanner<'e>                 NEW - mode + engine handle
      SECRET_SCAN_ENV             NEW - "GCM_SECRET_SCAN" const
      pub mod detect;             src/privacy/detect.rs   (unchanged body)
      pub mod entropy;            src/privacy/entropy.rs  (unchanged)
      pub mod rules;              src/privacy/rules.rs    (unchanged, include_str! corpus)

  [[bin]] gcm (required-features = ["cli"]) -> src/main.rs
    #[path = "privacy/facade.rs"] mod privacy;   src/privacy/facade.rs  NEW FILE
      Privacy                     moved from src/privacy/mod.rs
      PathFilter, IgnorePattern   moved from src/privacy/mod.rs
    mod cache, cli, config, debug, diff, error, git, output,
        paths, plan, provider, resolve, status, ui           (unchanged)
```

`src/privacy/facade.rs` is not declared by `src/privacy/mod.rs`, so cargo never compiles it into the library. The binary reaches it through a `#[path]` attribute, which preserves the `privacy::Privacy` name that ADR-002 lists as binary-only and keeps every existing `use crate::privacy::Privacy` callsite in `src/resolve/mod.rs` and `src/main.rs` untouched.

### Data flow

Two callers, one detection implementation:

```
  external crate (lok)                     gcm binary
  --------------------                     ----------
  gcm::privacy::rules::vendored()          Privacy::load(&repo, cli_mode)
          |                                        |
          |                                +-------+-------+
          |                                |               |
          |                        PathFilter::load    SecretScanMode::resolve_with(
          |                        (.gcmignore)          cli, |k| std::env::var(k).ok())
          |                                |               |
          v                                v               v
  Scanner::new(mode, engine) <----------- Privacy { filter, scanner: Scanner<'static> }
          |                                        |
          v                                        v
  scanner.scan(text) -> Result<String, ScanError>  Privacy::scan_text
          |                                        |  (delegates to scanner.scan,
          |                                        |   then ScanError -> GcmError)
          v                                        v
  detect::secret_ranges / detect::redact_secrets over &RuleEngine
```

Behaviour parity between the CLI and library paths is structural, not asserted: `Privacy::scan_text` becomes a two-line delegation to `Scanner::scan`, so there is only one copy of the off/redact/abort branch.

### Source paths

| Path | Change |
|---|---|
| `Cargo.toml` | `[lib]` section, `[features]`, `required-features` on `[[bin]]`, optional deps |
| `src/lib.rs` | **new** - crate docs, `pub mod privacy;` |
| `src/privacy/mod.rs` | keeps `SecretScanMode`; loses `Privacy`/`PathFilter`/`IgnorePattern`; gains `ScanError`, `Scanner`, `SECRET_SCAN_ENV`; `detect`/`entropy`/`rules` become `pub mod` |
| `src/privacy/facade.rs` | **new** - `Privacy`, `PathFilter`, `IgnorePattern`, `normalize_path`, `wildcard_match` moved verbatim |
| `src/privacy/detect.rs` | unchanged body; now reachable as `gcm::privacy::detect` |
| `src/privacy/entropy.rs` | unchanged |
| `src/privacy/rules.rs` | unchanged; `rules.toml` still embedded via `include_str!` |
| `src/error.rs` | `impl From<ScanError> for GcmError` |
| `src/main.rs` | `mod privacy;` becomes the `#[path]` facade declaration |
| `src/cli.rs` | `use crate::privacy::SecretScanMode` becomes `use gcm::privacy::SecretScanMode` |
| `src/resolve/mod.rs` | `use crate::privacy::{Privacy, SecretScanMode}` splits across the two crates |
| `tests/library_api.rs` | **new** - external-consumer integration test |

### Type identity

ADR-002 Implementation Note 1 is the trap to avoid: with both targets in one package, a type declared in both is two distinct types to the compiler. The binary therefore must not declare `mod privacy;` over `src/privacy/mod.rs`. It declares the facade file instead and imports `SecretScanMode` from `gcm::privacy`, so the enum the `--secret-scan` flag parses is the same enum `Scanner` matches on.

### Cargo.toml

```toml
[lib]
name = "gcm"
path = "src/lib.rs"

[[bin]]
name = "gcm"
path = "src/main.rs"
required-features = ["cli"]

[features]
default = ["cli"]
# Everything the binary needs and no library consumer does.
cli = ["clap", "dep:cliclack", "dep:console", "dep:ureq", "dep:sha2", "dep:which", "dep:url"]
# Opt-in ValueEnum derives for a consumer that does build a CLI.
clap = ["dep:clap"]

[dependencies]
clap = { version = "4", features = ["derive"], optional = true }
cliclack = { version = "0.5", default-features = false, optional = true }
console = { version = "0.16", default-features = false, features = ["std"], optional = true }
ureq = { version = "3", optional = true }
sha2 = { version = "0.11.0", optional = true }
which = { version = "8", optional = true }
url = { version = "2", optional = true }
# Library-relevant, stay non-optional:
regex = "1"        # detect.rs, rules.rs
serde = { version = "1", features = ["derive"] }   # rules.rs
toml = "0.8"       # rules.rs
serde_json = "1"   # binary today, needed by status in CLO-597
tempfile = "3"     # binary modules and the tests/ suite
```

`required-features = ["cli"]` is what makes the gating work without a single `#[cfg]` in the binary sources: under `--no-default-features` cargo simply does not build the `[[bin]]` target, so code referencing `clap` or `ureq` is never compiled. `ureq`, `sha2`, `which`, and `url` join the `cli` set because every one of their callsites (`src/provider/http.rs`, `src/config.rs`, `src/cache.rs`, `src/resolve/remote/host.rs`, `src/resolve/mergiraf.rs`) is binary-only. `serde_json` and `tempfile` stay non-optional deliberately: `serde_json` is required by the `status` types CLO-597 will export, and `tempfile` is used across `tests/`, where gating it would force a `required-features` entry on every existing integration test target.

---

## Public API surface

### `src/lib.rs`

```rust
//! gcm as a library: types and pure functions shared with in-org consumers.
//!
//! The CLI lives in the `gcm` binary target; this crate carries no `clap`,
//! `cliclack`, or HTTP transport. See `docs/adrs/002-library-boundary.md`.
//!
//! # Scanning text for secrets
//!
//! ```
//! use gcm::privacy::{rules, ScanError, Scanner, SecretScanMode};
//!
//! let engine = rules::vendored()?;
//! let scanner = Scanner::new(SecretScanMode::Redact, engine);
//! let clean = scanner.scan("token=ghp_abcdefghijklmnopqrstuvwxyz123456".to_string())?;
//! assert!(clean.contains("[REDACTED: secret]"));
//! # Ok::<(), ScanError>(())
//! ```

pub mod privacy;
```

### `src/privacy/mod.rs`

```rust
pub mod detect;
pub mod entropy;
pub mod rules;

/// Environment variable consulted by [`SecretScanMode::resolve_with`].
pub const SECRET_SCAN_ENV: &str = "GCM_SECRET_SCAN";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "clap", value(rename_all = "lower"))]
pub enum SecretScanMode {
    Off,
    Redact,
    Abort,
}

impl SecretScanMode {
    /// Resolve the effective mode: an explicit choice wins, otherwise the
    /// caller's environment lookup for `GCM_SECRET_SCAN`, otherwise `Off`.
    /// Never reads the process environment itself.
    pub fn resolve_with<F>(explicit: Option<Self>, env_lookup: F) -> Result<Self, ScanError>
    where
        F: FnOnce(&str) -> Option<String>;

    /// Parse one of `off` / `redact` / `abort`, case- and space-insensitive.
    /// An empty string is `Off`.
    pub fn parse(raw: &str) -> Result<Self, ScanError>;
}

/// Errors raised by the scanner. Carries no gcm domain types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanError {
    /// Abort mode found credential-looking content before egress.
    SecretDetected { count: usize },
    /// A mode string was not `off`, `redact`, or `abort`.
    InvalidMode { value: String },
    /// A rule pack failed to parse or compile.
    RulePack { message: String },
}

impl std::fmt::Display for ScanError {}
impl std::error::Error for ScanError {}

/// A scan mode bound to a compiled rule engine. `'e` is `'static` when the
/// engine comes from [`rules::vendored`].
#[derive(Clone, Copy, Debug)]
pub struct Scanner<'e> {
    mode: SecretScanMode,
    engine: &'e rules::RuleEngine,
}

impl<'e> Scanner<'e> {
    pub fn new(mode: SecretScanMode, engine: &'e rules::RuleEngine) -> Self;

    /// Convenience constructor over the vendored corpus.
    pub fn vendored(mode: SecretScanMode) -> Result<Scanner<'static>, ScanError>;

    pub fn mode(&self) -> SecretScanMode;
    pub fn engine(&self) -> &'e rules::RuleEngine;

    /// `Off` returns the text untouched, `Redact` replaces every detected
    /// range, `Abort` returns [`ScanError::SecretDetected`] when the count
    /// is non-zero.
    pub fn scan(&self, text: String) -> Result<String, ScanError>;

    /// Detected byte ranges, regardless of mode.
    pub fn ranges(&self, text: &str) -> Vec<std::ops::Range<usize>>;
}
```

### Re-exported unchanged (already `pub`, now reachable off-crate)

```rust
// gcm::privacy::rules
pub struct CompiledRule { pub id: String, pub regex: regex::Regex,
                          pub keywords: Vec<String>, pub entropy: Option<f64>,
                          pub min_digits: Option<u32> }
pub struct RuleEngine { /* private: RegexSet + Vec<CompiledRule> */ }
impl RuleEngine {
    pub fn compile(toml_src: &str) -> Result<Self, String>;
    pub fn matching_rules(&self, text: &str) -> impl Iterator<Item = &CompiledRule>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
pub fn vendored() -> Result<&'static RuleEngine, String>;

// gcm::privacy::detect
pub fn secret_ranges(text: &str, engine: &RuleEngine) -> Vec<Range<usize>>;
pub fn redact_secrets(text: &str, engine: &RuleEngine) -> String;
pub fn merge_ranges(ranges: Vec<Range<usize>>) -> Vec<Range<usize>>;

// gcm::privacy::entropy
pub enum Charset { Hex, Alnum, Base64 }
impl Charset {
    pub fn classify(value: &str) -> Self;
    pub fn normalized_floor(self) -> f64;
}
pub fn shannon_entropy(value: &str) -> f64;
pub fn normalized_entropy(value: &str) -> f64;
```

The `#[allow(dead_code)]` attributes currently on `CompiledRule::id`, `CompiledRule::keywords`, `RuleEngine::len`, and `RuleEngine::is_empty` come off: once the items are crate-public they are used by definition.

### Before / after on changed signatures

```rust
// SecretScanMode resolution: process env and GcmError leave the shared path.
- pub fn resolve(cli: Option<Self>) -> Result<Self, GcmError>      // reads std::env::var
+ pub fn resolve_with<F>(explicit: Option<Self>, env_lookup: F) -> Result<Self, ScanError>
+ where F: FnOnce(&str) -> Option<String>

- fn parse_env(raw: &str) -> Result<Self, GcmError>                // private
+ pub fn parse(raw: &str) -> Result<Self, ScanError>

// The binary facade keeps its signature; only its body and module path change.
  pub fn scan_text(&self, text: String) -> Result<String, GcmError>   // src/privacy/facade.rs
  pub fn load(repo: &Repo, cli_secret_scan: Option<SecretScanMode>) -> Result<Self, GcmError>
```

### Binary-side mapping (`src/error.rs`)

```rust
impl From<gcm::privacy::ScanError> for GcmError {
    fn from(e: gcm::privacy::ScanError) -> Self {
        match e {
            ScanError::SecretDetected { count } => GcmError::SecretDetected { count },
            other => GcmError::Config(other.to_string()),
        }
    }
}
```

`ScanError::InvalidMode`'s `Display` reproduces today's text verbatim (`unknown GCM_SECRET_SCAN value '<value>'. Use off, redact, or abort.`) so the CLI error message for a bad `GCM_SECRET_SCAN` is byte-identical to v0.5.2.

---

## Assumptions

- **A1 (high)** `src/privacy/{detect,entropy,rules}.rs` reference nothing outside `regex`, `serde`, `toml`, `std`, and each other, so they move into the library with zero body edits. Verified by reading their imports; re-verified by `cargo build --lib --no-default-features` compiling clean.
- **A2 (high)** Integration tests under `tests/` link the `[lib]` target as an external crate would, so `tests/library_api.rs` is a faithful stand-in for `lok`. Verification: the test compiles while naming only `gcm::privacy::*` and `std` types.
- **A3 (medium)** `required-features = ["cli"]` on `[[bin]]` is sufficient to keep `clap`/`cliclack`/`ureq` out of the library build without any `#[cfg]` in binary sources. Verification: `cargo build --no-default-features` succeeds and builds only the lib; `cargo tree --no-default-features -e normal` lists none of them.
- **A4 (medium)** Moving `ureq`, `sha2`, `which`, and `url` into the `cli` feature breaks nothing, because every callsite is in a binary-only module. Verification: full `cargo test` on default features stays green.
- **A5 (medium)** `default-run = "gcm"` coexists with `required-features` on the same target. `cargo run --no-default-features` will fail (no bin built) and that is acceptable. Verification: `cargo run -- --version` and `cargo install --path .` still work.
- **A6 (high)** No crate depends on gcm today (ADR-002 Decision 7, path-only), so turning `parse_env` into public `parse` and dropping `resolve` breaks no external consumer.
- **A7 (high)** The 475-test baseline in PRD AC1 is the default-feature `cargo test` count on `main @ bf8e395`. Verification: record the count before the first commit and compare after; new tests only add to it.
- **A8 (low)** One `ScanError` enum covering detection, mode parsing, and rule-pack failures will still fit when CLO-596 and CLO-597 add their own library errors. Verification path: revisit when those tickets define `provider::ProviderError` and the status resolution error on the library side.

---

## Test plan

### Library unit tests (`src/privacy/mod.rs`, `#[cfg(test)]`)

| Test fn | Asserts |
|---|---|
| `parse_accepts_all_modes_case_insensitively` | `off`/`OFF`/` redact `/`abort` and `""` map as today |
| `parse_rejects_unknown_value` | `ScanError::InvalidMode`, and the `Display` string matches the v0.5.2 wording |
| `resolve_with_prefers_explicit_over_env` | explicit `Off` wins over a lookup returning `abort` |
| `resolve_with_reads_caller_env_map` | closure over a `HashMap` returning `abort` yields `Abort` |
| `resolve_with_defaults_off_when_absent` | closure returning `None` yields `Off` |
| `resolve_with_never_touches_process_env` | closure that panics if asked for anything but `GCM_SECRET_SCAN`; explicit mode set, closure never invoked |
| `scanner_off_is_identity` | input string returned unchanged |
| `scanner_redact_replaces_every_match` | ports the existing `redacts_common_secret_shapes` assertions through `Scanner` |
| `scanner_abort_reports_detection_count` | `Err(ScanError::SecretDetected { count: 1 })` |
| `scanner_ranges_match_detect_secret_ranges` | `Scanner::ranges` equals `detect::secret_ranges` for the same input |

Existing tests in `detect.rs`, `entropy.rs`, and `rules.rs` move with their modules unchanged.

### Binary unit tests (`src/privacy/facade.rs`, `#[cfg(test)]`)

Moved verbatim from `src/privacy/mod.rs`: `wildcard_matches_basename_and_paths`, `filter_excludes_builtin_and_original_rename_path`, and `abort_mode_rejects_secret_text` (still asserting `GcmError::SecretDetected { count: 1 }`, now produced through the `From<ScanError>` mapping). One addition:

- `scan_error_maps_to_gcm_error` in `src/error.rs`: `SecretDetected` maps to `GcmError::SecretDetected`, `InvalidMode` and `RulePack` map to `GcmError::Config` preserving the message.

### Integration test (`tests/library_api.rs`) - the external-consumer view

- `external_consumer_compiles_engine_and_scans_text` - `rules::vendored()`, then `detect::secret_ranges` and `detect::redact_secrets` on a fixture string. Every helper the test defines takes and returns only `std` types and `gcm::privacy` types, which is the compile-time form of PRD AC2.
- `abort_mode_from_caller_supplied_env_map` - build a `HashMap<String, String>` with `GCM_SECRET_SCAN=abort`, resolve through `SecretScanMode::resolve_with(None, |k| map.get(k).cloned())`, scan secret-bearing text, assert `ScanError::SecretDetected`. The test never sets a process variable, which is PRD AC3.
- `custom_rule_pack_compiles_and_matches` - `RuleEngine::compile` on a small inline TOML pack, proving a consumer is not locked to the vendored corpus.
- `scan_error_is_std_error` - `fn takes_error(_: &dyn std::error::Error)` accepts a `ScanError`, so consumers can fold it into `anyhow`/`thiserror` stacks.
- The `src/lib.rs` doctest runs as part of `cargo test` and doubles as the copy-paste example for consumers.

### Feature and target matrix

No `Provider` trait work is in scope (ADR-002 Decision 5), so there is no per-backend matrix. The equivalent axis here is the feature and target matrix, and every row must be green before merge:

| Command | Builds | Expectation |
|---|---|---|
| `cargo build` | lib + bin | unchanged contributor workflow |
| `cargo build --no-default-features` | lib only | succeeds, bin skipped by `required-features` |
| `cargo test` | everything | 475 pre-existing tests plus the new ones, all passing |
| `cargo test --no-default-features --lib` | lib unit tests | scanner tests pass with no `clap` in the graph |
| `cargo test --no-default-features --test library_api` | lib + one integration test | external-consumer surface works without `cli` |
| `cargo tree --no-default-features -e normal` | - | output contains no `clap`, `cliclack`, `console`, or `ureq` (PRD AC5) |
| `cargo clippy --all-targets -- -D warnings` | - | clean |
| `cargo clippy --no-default-features -- -D warnings` | - | clean, catches dead code that only the binary was using |
| `cargo fmt --check` | - | clean |

### Manual verification (parity with v0.5.2)

1. Keep a v0.5.2 binary on hand (`cargo install --version 0.5.2 gcm --root /tmp/gcm-v052` or the current Homebrew binary).
2. In a scratch repo, stage a file containing `AWS=AKIAABCDEFGHIJKLMNOP` and a GitHub PAT-shaped token.
3. Run `gcm --secret-scan=abort --dry-run` on both binaries: identical stderr message, identical exit code, index left unstaged in the same state.
4. Run `gcm --secret-scan=redact --dry-run` on both: identical redacted diff sent to the provider (compare with `GCM_DEBUG` output).
5. Run `GCM_SECRET_SCAN=panic gcm` on both: identical `unknown GCM_SECRET_SCAN value` error.
6. Run `gcm resolve` on a conflicted fixture with `--secret-scan=abort` to exercise the hunk pre-scan at `src/resolve/mod.rs:694`.
7. Consumer smoke test: in a scratch crate, `gcm = { path = "../gcm", default-features = false }`, call `rules::vendored()` and `Scanner::scan`, then confirm `cargo tree` shows no `clap`.

---

## Migration / rollout

**For external consumers this is purely additive.** Nothing depends on gcm as a library today (ADR-002 Decision 7), so the new `[lib]` target creates a surface where there was none. No consumer-facing breakage is possible.

**For gcm itself the change is internal churn with no user-facing effect.** The CLI keeps every flag, default, message, and exit code. `default = ["cli"]` means `cargo build`, `cargo test`, `cargo install`, and the release pipeline behave exactly as before without passing any feature flags. The churn is confined to five files: `src/main.rs` (module declaration), `src/cli.rs` (one import), `src/resolve/mod.rs` (one import split across two crates), `src/error.rs` (one `From` impl), and the `src/privacy/` split itself.

**Rollout order** (each step compiles and tests green before the next):

1. `Cargo.toml`: `[lib]`, `[features]`, optional deps, `required-features` on `[[bin]]`. Add `src/lib.rs` with `pub mod privacy;` and no other change. At this point the binary still owns `Privacy` through the same module, so this step is a build-system-only change.
2. Split `src/privacy/mod.rs`: move `Privacy`, `PathFilter`, `IgnorePattern`, `normalize_path`, `wildcard_match`, and their tests into `src/privacy/facade.rs`; promote `detect`/`entropy`/`rules` to `pub mod`; declare the facade from `src/main.rs` via `#[path]`.
3. Add `ScanError` and `Scanner`, rewrite `SecretScanMode::resolve` as `resolve_with`, add `From<ScanError> for GcmError`, and reduce `Privacy::scan_text` to a delegation.
4. Migrate imports: `src/cli.rs` and `src/resolve/mod.rs` take `SecretScanMode` from `gcm::privacy`.
5. Add `tests/library_api.rs`, the `src/lib.rs` doctest, and the README library section.
6. Run the full feature matrix above.

**Feature flags:** `cli` (default on, gates the binary and its dependencies) and `clap` (off by default, gates the `ValueEnum` derive per ADR-002 Decision 4). A consumer that wants gcm's scanner in its own CLI enables `clap` alone; a background service enables neither.

**Version:** the package gains a public library surface, so 0.5.2 goes to 0.6.0 rather than 0.5.3, keeping room for CLO-596 and CLO-597 to extend the surface additively within the 0.6.x line.

**Consumer onboarding:** `lok` and `remem-ai` add `gcm = { path = "...", default-features = false }`. No publish step, no registry entry, no semver policy until an out-of-org consumer asks (ADR-002 Decision 7).

**Rollback:** every step is a self-contained commit. Reverting the `Cargo.toml` and `src/lib.rs` commits restores the single-target build; the `src/privacy/` split is a pure file move and reverts cleanly.

---

## Open questions

1. **Is `Scanner` the right shape, or should the library expose only free functions?** A `Scanner<'e>` handle keeps the off/redact/abort branch in one place, which is what makes CLI-versus-library parity structural rather than test-enforced, and it gives consumers a single object to hold. The cost is one more type on a surface ADR-002 described purely in terms of `secret_ranges`/`redact_secrets`/`merge_ranges`. The alternative is a free `pub fn scan_text(text: String, mode: SecretScanMode, engine: &RuleEngine) -> Result<String, ScanError>` with no new type, at the price of consumers threading two arguments through their own code. This design proposes `Scanner`; it is reversible either way before the first consumer lands.
2. **Should `CompiledRule` keep public `regex::Regex` fields?** ADR-002 lists `CompiledRule` on the exported surface, and the field is public today. Exporting it as-is puts `regex` in gcm's public API, so a `regex` major bump becomes a gcm breaking change. Sealing the fields behind accessors, or exporting `RuleEngine` without `CompiledRule`, avoids that but removes the rule-attribution data a consumer might want for logging which rule fired. Unresolved: whether any consumer actually needs per-rule introspection.
3. **Does `RuleEngine::compile` on a caller-supplied TOML pack become supported API?** It is `pub` today and the library boundary makes it callable off-crate, which effectively ships custom rule packs as a feature. PRD section 6 puts new rule syntax out of scope, but it says nothing about compiling caller-authored packs with the existing syntax. If this is supported, the TOML schema needs documenting and versioning; if not, `compile` should be crate-private with only `vendored` exported.
4. **Should the library keep a process-env convenience constructor behind the `cli` feature?** PRD S2 removes the process-env read from the shared path, and this design pushes the `|k| std::env::var(k).ok()` closure into the binary facade. A `#[cfg(feature = "cli")] pub fn resolve_from_process_env` would spare every CLI-shaped consumer from writing the same closure, at the cost of two resolution entry points to keep in sync.
5. **How far should dependency gating go in this ticket?** This design moves `ureq`, `sha2`, `which`, and `url` into the `cli` feature alongside the three ADR-mandated crates, and leaves `serde_json` and `tempfile` non-optional (CLO-597 will need `serde_json`; gating `tempfile` would require `required-features` entries on all six existing integration test targets). PRD AC5 only demands `clap` and `cliclack` exclusion, so the extra gating is discretionary scope that could equally be deferred to CLO-596.
6. **Does the `#[path = "privacy/facade.rs"]` declaration read well enough to keep?** It preserves the `privacy::Privacy` name from ADR-002 and leaves every existing callsite untouched, but `#[path]` is unusual in this codebase. The alternative, a top-level `src/privacy_facade.rs` with `mod privacy_facade;`, is more conventional and costs a rename at four callsites in `src/main.rs` and `src/resolve/mod.rs`.
