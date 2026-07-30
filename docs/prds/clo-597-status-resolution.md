# PRD: CLO-597 — Expose source-attributed status resolution through the gcm library

**Linear**: https://linear.app/cloud-ai/issue/CLO-597
**ADR**: [ADR-002](../adrs/002-library-boundary.md) — library boundary decisions
**Predecessor**: [CLO-596](https://linear.app/cloud-ai/issue/CLO-596) — provider identity + model registry (Done, PR #48)

## Problem

A downstream consumer (lok, remem-ai) wants to ask "what will gcm do right now, and why" — getting every key, model, and endpoint value back with the source it came from. Today `src/status.rs` (1,010 lines) is binary-only: its only entry point (`run_status_subcommand`) takes `&Cli` (a `clap` struct), and the module imports `crate::config::{self, Config}` and `crate::output::SCHEMA_VERSION`, neither of which is in the library yet. The attribution helpers are already env-pure (they take an `env_lookup` closure), but they are not crate-pure — they reference binary-only types.

## Users

- **Downstream library consumers** (lok, remem-ai): need programmatic access to status resolution without `clap`, `cliclack`, or a config file on disk.
- **The gcm binary**: must produce byte-identical `gcm status` / `gcm status --json` output after the extraction.

## Requirements

1. Library entry point for status resolution taking `Config` plus `env_lookup`, returning the report structs (`StatusReport`, `PathsStatus`, `ProviderStatus`).
2. `Cli` and `SCHEMA_VERSION` decoupled from the resolution path — they stay in the binary.
3. `Config` types cross the library boundary in the shape ADR-002 Decision 3 chose — data types + pure functions in the library, `cliclack` wizard in the binary.
4. Attribution helpers made public at the library boundary.
5. `ollama::is_cloud_model`, `ollama::normalize_host`, `ollama::DEFAULT_BASE_URL` move to the library (pure helpers, no HTTP dependency).

## Acceptance Criteria

- [ ] 522 tests still pass (was 475 at issue write time; CLO-596 added tests), with `tests/status.rs` (17 tests) covering the same behaviour
- [ ] `gcm status` and `gcm status --json` produce byte-identical output to v0.6.0 for the same config and environment
- [ ] A test resolves a full status report through the library using a caller-supplied env map, with no gcm config file on disk
- [ ] The library's status path pulls in neither `clap` nor `cliclack`
- [ ] `cargo test --no-default-features` compiles the library status + config surface

## Out of scope

- No extraction of `run_status_subcommand` (stays in the binary, takes `&Cli`)
- No change to `gcm status` output format or exit codes
- No extraction of `output.rs` (commit envelope, `SCHEMA_VERSION` stays binary-side)
- No crates.io publish (ADR-002 Decision 7)