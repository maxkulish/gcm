# Design: CLO-496 - Add first-run onboarding wizard for provider setup

**Status**: Finalized
**Finalized**: 2026-06-22
**Approved By**: Max Kulish

## Problem

First-time users of `gcm` are affected. Today the tool assumes the user has already exported a provider API key (`GROQ_API_KEY`, `GEMINI_API_KEY`, `OPENAI_API_KEY`, or `ANTHROPIC_API_KEY`) and optionally `GCM_PROVIDER` before the very first run; there is no persistent config file, no guided setup, and no way to re-run setup. As the discovery report records, a missing key surfaces only as a fatal `ProviderError::MissingKey` at call time, and the `--help` text (`EGRESS_DISCLOSURE` in `src/cli.rs`) is the only place the required env vars are documented. The result is a poor first-run experience: the user must read help output, guess the right env var for their provider, export it, and retry. This matters now because the two prerequisites called out in discovery - the config-format decision (CLO-485) and the provider registry (CLO-489, the `ProviderId` enum and `provider::select`) - are both complete, so the only remaining gap is config persistence plus an onboarding path. The fix is to detect an unconfigured first run, launch an interactive wizard that activates providers, captures keys or the Ollama endpoint, chooses a default, and persists the result with `0600` permissions - while degrading cleanly to printed instructions and a non-zero exit in a non-TTY context so unattended/CI use stays safe.

## Goals / Non-goals

**Goals**

- A new `src/config.rs` module that reads/writes a per-user config file (`config.toml`) in the OS config dir via the already-present `directories` crate, mirroring the `cache_dir()` pattern in `src/cache.rs`.
- Auto-detect an unconfigured first run (no config file and no provider hint in the environment) and launch an interactive wizard.
- A wizard that: offers the five v1 providers (Groq, Google/Gemini, Anthropic, OpenAI, Ollama), enables one or more, captures each cloud key from env when present else prompts with echo disabled, **probes** the Ollama daemon at `http://localhost:11434` (honoring `OLLAMA_HOST`, surfacing an actionable message when unreachable, per ADR-001 Decision 8), and lets the user pick a default from the enabled set.
- Persist config atomically with `0600` permissions (reusing the `write_atomic` / `open_private` approach from `src/cache.rs`); never copy a key that already lives in an env var into the file.
- A `gcm config` subcommand and a `--reconfigure` flag that re-run the wizard idempotently, overwriting the existing config without corrupting it.
- In a non-TTY context where onboarding would be required, print the needed config template plus the env vars to export and exit non-zero; never hang on a closed stdin.
- Preserve the existing precedence (`--provider`/`--model`/env) exactly; config becomes a fallback layer between env and the hardcoded default, not a replacement.

**Non-goals**

- Cloud key validation (pinging each provider) - explicitly out of scope per the PRD (carried as discovery debt that may resurface in review).
- Encrypted at-rest storage - permissions-only (`0600`) for v1.
- Automatic git alias installation (`git commit-ai`) - owned by CLO-497.
- Persisting model overrides or per-provider base URLs (beyond the Ollama endpoint) - the env layer already covers these; the file stays minimal per the PRD.
- Windows-specific secret entry - the existing supported platforms are macOS and Linux (see the shell-out comment in `src/ui.rs`).
- Rewriting `provider::select` or any backend - the change is additive.

## Architecture

A single new module, `src/config.rs`, owns the on-disk schema, load/save, first-run detection, the interactive wizard, and a thin "hydrate env from config" shell. The provider layer (`src/provider/mod.rs`) and every backend stay untouched: config makes itself visible by populating the env vars the providers already read lazily, so env precedence is preserved by construction (a pre-set var is never overwritten).

New code lands in:

- `src/config.rs` - the entire module (`Config`, `ProviderConfig`, load/save, `needs_onboarding`, `apply_to_env`, `run_wizard`, `non_tty_instructions`, `read_secret`).
- `src/main.rs` - a pre-step (`ensure_configured`) called before `provider::select`, plus handling of the new subcommand/flag.
- `src/cli.rs` - the optional `Commands` subcommand and the `--reconfigure` flag.
- `src/error.rs` - one new `GcmError::OnboardingRequired` variant.
- `src/provider/mod.rs` - one additive method, `ProviderId::key_env_var`, and serde derives on `ProviderId` (signature change shown below).

Data flow:

```
                         gcm <args>
                             |
              +--------------+---------------+
              |                              |
   `config` subcommand / --reconfigure   normal run
              |                              |
        run_wizard()                  config::load() -> Option<Config>
              |                              |
        config::save()              +--------+-----------------+
              |                     |                          |
        print + exit 0      Some(cfg): apply_to_env(cfg)   None:
                                    |                     needs_onboarding(flag)?
                                    |                       |          |
                                    |                      no         yes
                                    |                       |          |
                                    |                       |    TTY?--+--non-TTY
                                    |                       |    |          |
                                    |                       |  wizard   OnboardingRequired
                                    |                       |  +save     (print template,
                                    |                       |  +apply     exit non-zero)
                                    +-----------+-----------+--+
                                                |
                                       provider::select(flag, model)   (unchanged)
                                                |
                                       existing commit flow
```

`apply_to_env` is the load-time bridge: for each enabled cloud provider that stored an inline key, it sets the provider's key env var only if unset; for Ollama it sets `GCM_OLLAMA_BASE_URL` only if neither it nor `OLLAMA_HOST` is set; and it sets `GCM_PROVIDER` to `config.default` only if `--provider` is absent and `GCM_PROVIDER` is unset/blank. Because `provider::select` reads the flag first, then env, the flag still wins, env still wins over config, and the backends need no changes.

Concrete types (full signatures in the next section): `Config { version, default: ProviderId, providers: Vec<ProviderConfig> }` serialized as TOML array-of-tables; `ProviderConfig { id: ProviderId, key: Option<String>, endpoint: Option<String> }` where `key == None` means "read from the env var at run time" and `key == Some(_)` means an inline secret in the `0600` file (always `None` for key-free Ollama). A `version: u32` field mirrors `CacheFile`'s forward-compat versioning in `src/cache.rs`.

Example persisted file:

```toml
version = 1
default = "groq"

[[providers]]
id = "groq"
key = "<INLINE_SECRET_OR_OMITTED_WHEN_FROM_ENV>"

[[providers]]
id = "ollama"
endpoint = "http://localhost:11434"
```

## Public API surface

New module `src/config.rs`:

```rust
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::GcmError;
use crate::provider::ProviderId;

/// On-disk config format version (mirrors `cache::CacheFile.version`); a
/// mismatch on read is treated as "no usable config".
const CONFIG_FORMAT_VERSION: u32 = 1;

/// Persisted provider configuration, written as TOML to `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub version: u32,
    /// Provider used when neither `--provider` nor `GCM_PROVIDER` is set.
    pub default: ProviderId,
    /// Every provider the user enabled during onboarding.
    pub providers: Vec<ProviderConfig>,
}

/// One enabled provider. `key == None` => read from the provider env var at run
/// time (env-only); `key == Some(_)` => inline secret in the 0600 file. Always
/// `None` for key-free Ollama, which uses `endpoint` instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: ProviderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

/// `$GCM_CONFIG/config.toml` if the override is set (tests / relocation),
/// else the OS config dir via `directories` (mirrors `cache::cache_dir`).
/// `None` if no config dir can be determined. The override env var name
/// `GCM_CONFIG` is fixed by ADR-001 Decision 4.
pub fn config_path() -> Option<PathBuf>;

/// Load and parse the config, or `None` on absent / unreadable / unparseable /
/// wrong-version file (a miss, never an abort). A malformed TOML parse error
/// or a `default` not among `providers` also returns `None` — the caller
/// treats it as "needs onboarding" and prints a warning to stderr pointing
/// to the config file path. If the file exists but has world-readable
/// permissions (not `0600`), `load()` warns to stderr and returns `None`.
pub fn load() -> Option<Config>;

/// Persist atomically with 0600 permissions (reuses the cache write strategy:
/// a private temp file renamed over the target, so it is never world-readable).
/// The atomic rename means concurrent first-run processes are safe:
/// first-to-write wins, the second sees the config on its next `load()`.
pub fn save(config: &Config) -> std::io::Result<()>;;

/// True iff this is an unconfigured first run: no config file AND no provider
/// hint in the environment (no `--provider`, no non-blank `GCM_PROVIDER`, no
/// cloud key env var set). An env-configured user is never interrupted.
pub fn needs_onboarding(cli_provider: Option<ProviderId>) -> bool;

/// Bridge a loaded config into the (unchanged) provider layer by setting env
/// vars it has not already been given. Env always wins: a pre-set var is never
/// overwritten. Best-effort, returns nothing.
pub fn apply_to_env(config: &Config);

/// Run the interactive wizard end to end (enable providers, capture keys/
/// endpoint, choose default) and return the assembled, validated `Config`.
/// The Ollama daemon probe uses a 3-second connection timeout (per ADR-001
/// Decision 8) to avoid hanging the wizard on an unresponsive endpoint.
/// Empty key input is treated as env-only (`key: None`), not `Some("")`.
/// Invalid menu selections re-prompt instead of erroring.
pub fn run_wizard() -> Result<Config, GcmError>;

/// Pure assembly of a `Config` from collected answers (no I/O), so the wizard's
/// logic is unit-testable; `run_wizard` is the imperative shell around it.
/// Errors if `default` is not among the enabled providers.
pub fn build_config(
    enabled: &[ProviderConfig],
    default: ProviderId,
) -> Result<Config, GcmError>;

/// Render the non-TTY guidance: a `config.toml` template plus the `export`
/// lines for each provider's key env var.
pub fn non_tty_instructions() -> String;
```

Secret entry reuses the existing shell-out idiom from `src/ui.rs` (no new dependency):

```rust
/// Read one line from stdin with terminal echo disabled, best-effort via
/// `stty -echo` / `stty echo` (mirrors `ui::edit_in_editor`'s shell-out to
/// `sh`). Echo is always restored — an RAII guard runs `stty echo` on Drop,
/// so echo is restored even on Ctrl+C or panic. Echo is always restored; a
/// trailing newline is printed. Falls back to visible input if `stty` is
/// unavailable. An empty/whitespace-only input returns `Ok(String::new())`,
/// which the wizard interprets as "use env var, do not store inline".
fn read_secret(prompt: &str) -> std::io::Result<String>;
```

`ProviderId` in `src/provider/mod.rs` gains serde support (so it can be a TOML field) and a key-env-var accessor.

Before:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ProviderId {
    Groq,
    #[value(alias = "gemini")]
    Google,
    Openai,
    Anthropic,
    Ollama,
}
```

After:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[value(rename_all = "lower")]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Groq,
    #[value(alias = "gemini")]
    #[serde(alias = "gemini")]
    Google,
    Openai,
    Anthropic,
    Ollama,
}

impl ProviderId {
    /// The provider's API key env var, or `None` for key-free Ollama. Centralizes
    /// the mapping each backend currently holds as a private `API_KEY_ENV` const.
    pub fn key_env_var(self) -> Option<&'static str> {
        match self {
            ProviderId::Groq => Some("GROQ_API_KEY"),
            ProviderId::Google => Some("GEMINI_API_KEY"),
            ProviderId::Openai => Some("OPENAI_API_KEY"),
            ProviderId::Anthropic => Some("ANTHROPIC_API_KEY"),
            ProviderId::Ollama => None,
        }
    }
}
```

`src/cli.rs` gains an optional subcommand and a flag (existing flags unchanged; no-subcommand invocations still parse to the commit flow).

Before:

```rust
#[derive(Parser, Debug)]
#[command(name = "gcm", /* ... */)]
pub struct Cli {
    #[arg(long)]
    pub dry_run: bool,
    // ... existing flags ...
    #[arg(long)]
    pub model: Option<String>,
}
```

After:

```rust
#[derive(Parser, Debug)]
#[command(name = "gcm", /* ... */)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(long)]
    pub dry_run: bool,
    // ... existing flags unchanged ...
    #[arg(long)]
    pub model: Option<String>,

    /// Re-run the interactive provider setup wizard, then continue.
    #[arg(long)]
    pub reconfigure: bool,
}

#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    /// Run the interactive provider setup wizard and exit.
    Config,
}
```

`src/error.rs` gains one variant (shown before/after for the enum):

```rust
pub enum GcmError {
    NotARepo,
    Git(String),
    Provider(ProviderError),
    NonInteractive,
    Editor(String),
    EmptyMessage,
    UnmergedConflicts,
    CommitFailed(String),
    /// First-run setup is needed but there is no terminal to run the wizard.
    /// The caller prints `config::non_tty_instructions()` and exits non-zero.
    OnboardingRequired,
}
```

`OnboardingRequired::leaves_staged()` is `false` (it occurs before any staging) and its `Display` points the user to `gcm config` / the printed template.

## Assumptions

- `directories::ProjectDirs::from("", "", "gcm").config_dir()` yields a writable per-user config directory on macOS and Linux. Confidence: high. Verification: `src/cache.rs` already uses the sibling `cache_dir()` the same way; mirror it and cover with a `GCM_CONFIG` override test.
- Honoring the discovery-chosen TOML format requires adding the `toml` crate; `serde` + `serde_json` (the current deps) cannot parse TOML. Confidence: high. Verification: `Cargo.toml` lists no `toml` dependency. (See Open question 1 - this is the one new dependency the chosen approach implies.)
- Echo suppression via `stty -echo` / `stty echo` works on the supported platforms because the codebase already shells out to `sh` for the editor. Confidence: high. Verification: the platform comment and `Command::new("sh")` pattern in `src/ui.rs::edit_in_editor`.
- `std::env::set_var` is sound for the env-hydration bridge: the crate is edition 2021 and hydration runs once at startup, before any provider call or thread spawn. Confidence: medium. Verification: `Cargo.toml` `edition = "2021"`. If the crate later moves to edition 2024, `set_var` becomes `unsafe` and this bridge must be wrapped (or `provider::select` must take an explicit config).
- Clap accepts an optional `#[command(subcommand)] Option<Commands>` alongside the existing top-level flags, so `gcm` with no subcommand still runs the commit flow. Confidence: high. Verification: `cargo build` + a no-args parse test.
- Storing an inline key in a `0600` file satisfies the team's security bar for v1 (the PRD permits "0600 or env-only"). Confidence: medium. Verification: PRD requirement 10; the wizard minimizes inline secrets by recording env-only when the key is already exported. **Tension:** ADR-001 Decision 4 states "API keys are referenced by env-var name, never stored as plaintext in config" - the design defaults to env-var references but allows inline `0600` storage as a fallback when the key is not in env. See Open question 6.
- In `--json` mode, the onboarding-required early exit must emit a JSON error envelope on stdout, not prompt text or human instructions; all interactive prompts and human-facing diagnostics go to stderr (per `.pi/lessons/clo-493-...md § L1`). Confidence: high. Verification: existing `output::error` envelope pattern; add a `--json` non-TTY first-run integration test.

## Test plan

**Unit tests** (in `src/config.rs` under `#[cfg(test)] mod tests`, matching the inline-test convention used across the crate):

- `config_round_trips_toml` - serialize a `Config` and parse it back to an equal value.
- `config_parses_array_of_tables` - a hand-written `config.toml` sample (groq inline + ollama endpoint) deserializes to the expected `Config`.
- `key_none_is_env_some_is_inline` - `ProviderConfig.key` omitted from TOML parses to `None`; a present value parses to `Some`.
- `build_config_rejects_default_not_enabled` - `build_config` errors when `default` is not among `enabled`.
- `build_config_records_env_when_key_already_set` - the wizard core stores `key: None` for a provider whose key env var is already exported (no inline secret captured).
- `needs_onboarding_matrix` - no config + no env hint -> true; config present -> false; `--provider` set -> false; non-blank `GCM_PROVIDER` -> false; any single cloud key env set -> false.
- `apply_to_env_does_not_override_existing` - a pre-set key env var is left intact (env wins).
- `apply_to_env_sets_inline_key_endpoint_and_default` - an inline key sets the right env var, an Ollama endpoint sets `GCM_OLLAMA_BASE_URL` (only when `OLLAMA_HOST` is unset), and `config.default` sets `GCM_PROVIDER` only when unset.
- `config_path_honors_gcm_config_override` - `GCM_CONFIG` redirects the path (hermetic temp dir).
- `non_tty_instructions_lists_each_enabled_provider` - output contains a TOML template and an `export <KEY>=<VALUE>` line per enabled cloud provider.
- `provider_id_key_env_var_mapping` (in `src/provider/mod.rs`) - each variant maps to the right key env var; Ollama -> `None`.
- `provider_id_serde_round_trip_with_alias` - serde renders lowercase and parses the `gemini` alias to `Google`.
- `load_returns_none_on_malformed_toml` - a corrupt `config.toml` returns `None` (triggers onboarding, not an abort).
- `load_returns_none_on_default_not_in_providers` - a hand-edited config where `default` is not among `providers` returns `None`.
- `load_warns_on_world_readable_permissions` - a `0644` config file triggers a stderr warning and returns `None`.
- `build_config_treats_empty_key_as_env_only` - an empty/whitespace key input produces `key: None`, not `Some("")`.
- `ollama_probe_respects_timeout` - the daemon probe uses a 3-second timeout and does not hang on an unreachable endpoint.
- `ollama_endpoint_validates_url_format` - a non-URL endpoint (e.g. `not-a-url`) is rejected before persisting.
- `read_secret_restores_echo_on_drop` - the RAII guard runs `stty echo` even if the wizard is interrupted (tested via the guard's Drop impl).

**Per-provider test matrix** (the provider-config interaction has a per-backend dimension):

| Provider  | Key env var         | Prompted for key (env unset)? | Endpoint prompt?            | `apply_to_env` sets (when inline)        |
|-----------|---------------------|-------------------------------|-----------------------------|------------------------------------------|
| Groq      | `GROQ_API_KEY`      | yes                           | no                          | `GROQ_API_KEY`                           |
| Google    | `GEMINI_API_KEY`    | yes                           | no                          | `GEMINI_API_KEY`                         |
| OpenAI    | `OPENAI_API_KEY`    | yes                           | no                          | `OPENAI_API_KEY`                         |
| Anthropic | `ANTHROPIC_API_KEY` | yes                           | no                          | `ANTHROPIC_API_KEY`                      |
| Ollama    | (none)              | no                            | yes (default `localhost:11434`) | `GCM_OLLAMA_BASE_URL` (if non-default) |

**Integration tests** (new `tests/onboarding.rs`, driven by `std::process::Command` + `env!("CARGO_BIN_EXE_gcm")` with a temp `GCM_CONFIG`, no new test dependency):

- `first_run_non_tty_prints_instructions_and_exits_nonzero` - empty temp config dir, no key env vars, stdin from `/dev/null`; assert non-zero exit and stderr contains the TOML template and an `export` line.
- `first_run_json_non_tty_emits_envelope_not_prompts` - same setup but `--json`; assert stdout is a single valid JSON envelope with `status: error` and stderr (not stdout) contains the instructions (per `.pi/lessons/clo-493-...md § L1`).
- `existing_env_user_is_not_interrupted` - no config file but `GROQ_API_KEY` set; assert the run proceeds on the normal path (no onboarding error envelope).
- `existing_config_inline_key_hydrates_env` - write a `config.toml` with an inline key under the temp dir, run `gcm --dry-run` in a throwaway repo, and assert it does not fail with `MissingKey`.

**Manual verification:**

1. On a machine with no config and no key env vars, run `gcm` in a dirty repo, complete the wizard, and confirm a commit lands (PRD acceptance criterion 1).
2. Confirm `config.toml` exists with `0600` permissions (`ls -l`) and contains no key for any provider whose key was taken from env.
3. Re-run `gcm config` (and separately `gcm --reconfigure`), change selections, and confirm the file is overwritten cleanly with no duplicate `[[providers]]` entries (idempotency / no corruption).
4. Confirm secret entry does not echo to the terminal.
5. Pipe `gcm </dev/null` with no config and confirm the instructions print and the exit code is non-zero.
6. Pipe `gcm --json </dev/null` with no config and confirm stdout is a single JSON error envelope (status: error) and stderr contains the human instructions (per `.pi/lessons/clo-493-...md § L1`).
7. Run the pre-merge gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.

## Migration / rollout

This change is additive. An existing user who configures `gcm` purely through env vars is never interrupted: `needs_onboarding` returns false whenever `--provider`, a non-blank `GCM_PROVIDER`, or any cloud key env var is present, and `apply_to_env` never overwrites an env var that is already set, so the documented precedence (`flag > env > default`) is preserved exactly. Users with no config and no env hints get the new wizard on first run; in a non-TTY context they get printed instructions and a non-zero exit instead of a hang.

The only `Cargo.toml` change is adding the `toml` crate (the single dependency the chosen TOML approach requires - see Open question 1). No feature flag is needed; the behavior keys off the presence of a config file and the environment. A `version` field in `Config` (mirroring `CacheFile.version`) gives a forward-compatible migration hook should the schema change later; an unknown version reads as "no usable config" rather than an error. A `GCM_CONFIG` override is added for hermetic tests and relocation, per ADR-001 Decision 4 (which names `GCM_CONFIG` as the path override, mirroring `GCM_CACHE_DIR` for the cache).

Suggested rollout order:

1. Add `src/config.rs` (schema, load/save, `needs_onboarding`, `apply_to_env`, wizard, instructions) with unit tests; add the `toml` dependency.
2. Add `ProviderId` serde derives + `key_env_var`; add `GcmError::OnboardingRequired`.
3. Wire `src/cli.rs` (subcommand + `--reconfigure`) and the `ensure_configured` pre-step in `src/main.rs`.
4. Add `tests/onboarding.rs` and extend `scripts/acceptance.sh` with a non-TTY first-run check.
5. Update `README.md`: a "First-run setup" section, a `config.toml` / `GCM_CONFIG` row in the Configuration table, and a note that `gcm config` / `--reconfigure` re-runs setup to update keys and selections (key rotation UX).

## Resolved decisions (design checkpoint, 2026-06-22)

- **Open question 1 - TOML format vs the `toml` dependency: RESOLVED -> TOML + `toml` crate.** Honors discovery's `approach_chosen` and ADR-001 Decision 4 (format: TOML). The single new `toml` dependency is accepted for hand-editability and ecosystem-standard alignment; this is the only `Cargo.toml` addition.
- **Open question 6 - Inline key storage vs ADR-001 Decision 4: RESOLVED -> keep the `0600` inline fallback.** The wizard defaults to env-var references (`key: None`) and only stores an inline key (`key: Some(_)`) in the `0600` file when the user types a key not already in env, per the ADR's "if a value is ever written, the file is `0600`-equivalent" clause (FR-55). First-run UX outweighs strict env-only. README must document that the config file may hold a secret at `0600`.

## Open questions

1. ~~**TOML format vs the `toml` dependency.**~~ Resolved above: TOML + `toml` crate.
2. **`gcm config` subcommand vs `--reconfigure` flag.** Carried from discovery debt. The PRD requires both entry points, so the open part is the shape: introducing a real clap subcommand now changes the `--help` layout and sets a precedent for future subcommands, whereas a `--reconfigure` flag plus a thin `config` alias keeps the CLI flat. This design proposes both (subcommand + flag) but the structural choice (full subcommand tree vs flag-first) is genuinely open.
3. **Should `--provider X` with a missing key trigger the wizard, or keep today's fast `MissingKey` error?** Proposed: keep the existing error so onboarding only fires for the truly unconfigured first run (preserves backward-compat and CI safety), trading some discoverability. The alternative - launching the wizard whenever the resolved provider lacks a key - is more helpful but risks surprising scripted/env-driven users.
4. **Cloud key validation.** Out of scope per the PRD, but flagged in discovery debt as something that may resurface in review. Tradeoff: a lightweight ping catches a bad key at setup time (better UX) at the cost of a network call, provider-specific validation endpoints, and a slower wizard. Left out for v1.
5. **Windows secret entry.** Echo suppression relies on `stty`, which is absent on Windows; the existing supported platforms are macOS and Linux. Open: add a visible-input fallback with a warning, or document env-only configuration on Windows.
6. ~~**Inline key storage vs ADR-001 Decision 4.**~~ Resolved above: keep the `0600` inline fallback. The wizard defaults to env-var references and only stores an inline key (`0600`) when the user types a key not already in env, per the ADR's "if a value is ever written" clause (FR-55). README documents that the file may hold a secret at `0600`.
