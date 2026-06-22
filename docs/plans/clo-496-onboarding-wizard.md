# CLO-496 Implementation Plan: Add first-run onboarding wizard for provider setup

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-496
**Design Document**: docs/designs/clo-496-onboarding-wizard.md (Finalized 2026-06-22)
**Architecture Reference**: docs/adrs/001-foundational-architecture-decisions.md (Decisions 4 + 8)
**Created**: 2026-06-22
**Overall Progress**: 100% (91/91 tasks completed)

---

## Architecture Context

A single new module, `src/config.rs`, owns the on-disk schema, load/save, first-run detection, the interactive wizard, and an `apply_to_env` bridge. The provider layer and all backends stay untouched: config makes itself visible by populating the env vars providers already read lazily, so the documented precedence (`flag > env > config > default`) is preserved by construction. Two finalized decisions shape the work: config is **TOML** (new `toml` dependency, per ADR-001 Decision 4), and the wizard **defaults to env-var references** but may store an inline key in a `0600` file when the user types a key not already in env (FR-55). Non-TTY first runs print instructions and exit non-zero; `--json` mode emits a JSON error envelope on stdout with instructions on stderr (CLO-493 L1).

---

## Tasks

### Phase 1: Foundation (dependencies, provider + error plumbing)

- [x] Task 1: Add the `toml` crate dependency
  - [x] Add `toml` to `[dependencies]` in `Cargo.toml`
  - [x] Run `cargo build` to lock the version and confirm it resolves

- [x] Task 2: Extend `ProviderId` in `src/provider/mod.rs`
  - [x] Add `Serialize, Deserialize` derives with `#[serde(rename_all = "lowercase")]` and the `#[serde(alias = "gemini")]` on `Google`
  - [x] Add `pub fn key_env_var(self) -> Option<&'static str>` mapping each variant to its key env var (Ollama -> `None`)
  - [x] Unit test `provider_id_key_env_var_mapping`: each variant maps correctly, Ollama -> `None`
  - [x] Unit test `provider_id_serde_round_trip_with_alias`: renders lowercase, parses `gemini` alias to `Google`

- [x] Task 3: Add the `OnboardingRequired` error variant in `src/error.rs`
  - [x] Add `OnboardingRequired` to `GcmError` with a doc comment
  - [x] `Display` points the user to `gcm config` / the printed template
  - [x] Ensure `leaves_staged()` returns `false` for the variant (occurs before any staging)

### Phase 2: Config module core (schema, load/save, detection, env bridge)

- [x] Task 4: Create `src/config.rs` with the on-disk schema
  - [x] Define `Config { version: u32, default: ProviderId, providers: Vec<ProviderConfig> }`
  - [x] Define `ProviderConfig { id, key: Option<String>, endpoint: Option<String> }` with `skip_serializing_if = "Option::is_none"`
  - [x] Add `const CONFIG_FORMAT_VERSION: u32 = 1`
  - [x] Register `mod config;` in `src/main.rs`/`src/lib.rs` as appropriate

- [x] Task 5: Implement `config_path()` and `load()`
  - [x] `config_path()`: honor `GCM_CONFIG` override, else OS config dir via `directories` (mirror `cache::cache_dir`)
  - [x] `load()`: return `None` on absent/unreadable/unparseable/wrong-version file
  - [x] `load()`: return `None` + stderr warning on malformed TOML (triggers onboarding, never aborts)
  - [x] `load()`: return `None` + stderr warning when `default` not among `providers`
  - [x] `load()`: return `None` + stderr warning when file permissions are not `0600`

- [x] Task 6: Implement `save()` with atomic 0600 write
  - [x] Reuse the `write_atomic` / `open_private` strategy from `src/cache.rs` (private temp file renamed over target)
  - [x] Document first-to-write-wins concurrency semantics in the doc comment

- [x] Task 7: Implement `needs_onboarding()` and `apply_to_env()`
  - [x] `needs_onboarding(cli_provider)`: true only when no config AND no env hint (`--provider`, non-blank `GCM_PROVIDER`, any cloud key env var)
  - [x] `apply_to_env()`: set each inline provider key env var only if unset; set `GCM_OLLAMA_BASE_URL` only if it and `OLLAMA_HOST` unset; set `GCM_PROVIDER` to `config.default` only if unset/blank

- [x] Task 8: Phase 2 unit tests in `src/config.rs`
  - [x] `config_round_trips_toml`, `config_parses_array_of_tables`, `key_none_is_env_some_is_inline`
  - [x] `needs_onboarding_matrix` (no config/no env -> true; config/`--provider`/`GCM_PROVIDER`/any cloud key -> false)
  - [x] `apply_to_env_does_not_override_existing`, `apply_to_env_sets_inline_key_endpoint_and_default`
  - [x] `config_path_honors_gcm_config_override` (hermetic temp dir)
  - [x] `load_returns_none_on_malformed_toml`, `load_returns_none_on_default_not_in_providers`, `load_warns_on_world_readable_permissions`

### Phase 3: Interactive wizard

- [x] Task 9: Implement `read_secret()` with echo suppression
  - [x] Shell out to `stty -echo` / `stty echo` (mirror `ui::edit_in_editor`); fall back to visible input if `stty` unavailable
  - [x] RAII guard restores echo on Drop (Ctrl+C / panic safe); print trailing newline
  - [x] Empty/whitespace input returns `Ok(String::new())` (interpreted as env-only)
  - [x] Unit test `read_secret_restores_echo_on_drop` (via the guard's Drop impl)

- [x] Task 10: Implement `build_config()` (pure assembly, no I/O)
  - [x] Error when `default` not among `enabled`
  - [x] Empty key input produces `key: None`, not `Some("")`
  - [x] Unit tests `build_config_rejects_default_not_enabled`, `build_config_records_env_when_key_already_set`, `build_config_treats_empty_key_as_env_only`

- [x] Task 11: Implement the Ollama daemon probe
  - [x] Probe `http://localhost:11434` honoring `OLLAMA_HOST`, 3-second connection timeout (ADR-001 Decision 8)
  - [x] Surface an actionable message when unreachable; do not hang the wizard
  - [x] Validate the endpoint URL format before persisting
  - [x] Unit tests `ollama_probe_respects_timeout`, `ollama_endpoint_validates_url_format`

- [x] Task 12: Implement `run_wizard()` and `non_tty_instructions()`
  - [x] `run_wizard()`: enable providers, capture keys from env-or-prompt, choose default; re-prompt on invalid selections
  - [x] `non_tty_instructions()`: render a `config.toml` template plus `export <KEY>=<VALUE>` lines per enabled cloud provider
  - [x] Unit test `non_tty_instructions_lists_each_enabled_provider`

### Phase 4: CLI wiring and main pre-step

- [x] Task 13: Wire `src/cli.rs`
  - [x] Add `#[command(subcommand)] pub command: Option<Commands>` and `Commands::Config`
  - [x] Add `--reconfigure` flag; keep all existing flags unchanged
  - [x] No-args parse test: `gcm` with no subcommand still resolves to the commit flow

- [x] Task 14: Add `ensure_configured` pre-step in `src/main.rs`
  - [x] On `config` subcommand / `--reconfigure`: run wizard, save, print + exit 0
  - [x] Normal run: `load()` -> `apply_to_env()`; if `None` and `needs_onboarding`: TTY -> wizard+save+apply, non-TTY -> `OnboardingRequired`
  - [x] Call before `provider::select`; preserve precedence exactly
  - [x] Non-TTY `OnboardingRequired`: print `non_tty_instructions()` and exit non-zero; in `--json` emit a JSON error envelope on stdout, instructions on stderr (CLO-493 L1)

### Phase 5: Integration tests and acceptance script

- [x] Task 15: Add `tests/onboarding.rs` (process-driven, temp `GCM_CONFIG`)
  - [x] `first_run_non_tty_prints_instructions_and_exits_nonzero`
  - [x] `first_run_json_non_tty_emits_envelope_not_prompts` (stdout single JSON envelope, instructions on stderr)
  - [x] `existing_env_user_is_not_interrupted` (`GROQ_API_KEY` set, no config -> normal path)
  - [x] `existing_config_inline_key_hydrates_env` (inline-key config -> `gcm --dry-run` does not fail with `MissingKey`)

- [x] Task 16: Extend `scripts/acceptance.sh`
  - [x] Add a non-TTY first-run check (empty config dir, stdin from `/dev/null`, assert non-zero exit + instructions)

### Phase 6: Documentation

- [x] Task 17: Update `README.md`
  - [x] Add a "First-run setup" section
  - [x] Add a `config.toml` / `GCM_CONFIG` row to the Configuration table
  - [x] Note that `gcm config` / `--reconfigure` re-runs setup to update keys and selections (key rotation UX)
  - [x] Note that the config file may hold a secret at `0600` (per the inline-key decision)

### Phase 7: Testing & Validation

- [x] Task 18: Run the full pre-merge gate
  - [x] `cargo fmt --check`
  - [x] `cargo clippy -- -D warnings`
  - [x] `cargo test`
  - [x] `bash scripts/acceptance.sh`
- [x] Task 19: Manual verification (design doc § Test plan, items 1-7)
  - [x] No-config/no-env run completes the wizard and a commit lands
  - [x] `config.toml` exists with `0600` perms and no key for env-sourced providers
  - [x] `gcm config` and `gcm --reconfigure` overwrite cleanly (no duplicate `[[providers]]`)
  - [x] Secret entry does not echo
  - [x] `gcm </dev/null` and `gcm --json </dev/null` behave per spec

### Phase 8: Finalization

- [x] Task 20: Create PR with conventional commit messages
  - [x] Verify commits follow `type(CLO-496): description`
  - [x] Push branch: `git push origin feat/clo-496-onboarding`
  - [x] Create PR: `gh pr create --title "feat(CLO-496): add first-run onboarding wizard for provider setup" --body "..."`
  - [x] Link PR to Linear task CLO-496
  - [x] Request review

---

## Module Structure

- `src/config.rs` - new: schema, load/save, `needs_onboarding`, `apply_to_env`, `run_wizard`, `build_config`, `read_secret`, `non_tty_instructions`, Ollama probe
- `src/provider/mod.rs` - modified: `ProviderId` serde derives + `key_env_var`
- `src/error.rs` - modified: `GcmError::OnboardingRequired`
- `src/cli.rs` - modified: `Commands::Config` subcommand + `--reconfigure` flag
- `src/main.rs` - modified: `ensure_configured` pre-step, subcommand handling, `mod config`
- `tests/onboarding.rs` - new: process-driven integration tests
- `scripts/acceptance.sh` - modified: non-TTY first-run check
- `README.md` - modified: first-run setup + config docs
- `Cargo.toml` - modified: add `toml` dependency

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. The overall percentage will be recalculated based on completed tasks.

---

## Notes

- Keep the change additive; do not touch `provider::select` or any backend.
- Mirror existing patterns: `cache.rs` for atomic/private writes and path resolution, `ui.rs` for the `sh`/`stty` shell-out idiom.
- Inline tests under `#[cfg(test)] mod tests` per the crate convention.
- Mark tasks `[~]` when starting, `[x]` when complete.
