# Spec: `gcm status` - read-only config/provider introspection with source attribution

**Created**: 2026-06-26
**Linear**: [CLO-515](https://linear.app/cloud-ai/issue/CLO-515/add-gcm-status-command-to-show-active-providers-models-paths-and)
**Estimated scope**: M (5-6 files, ~6 sub-tasks)

## 1. Problem Statement

Today the only way to see what `gcm` will actually do is to read `~/.config/gcm/config.toml`
by hand and mentally replay the precedence rules. There is no single command that answers
"what is gcm going to do right now, and why". The value alone is not enough - a user needs
to see **where** each effective key/model came from (inline config, env var, or built-in
default), because gcm resolves these from layered sources.

Add a read-only `gcm status` subcommand that introspects the current configuration and
prints: gcm version, config paths and their source, and per-provider activation state,
effective model, and source attribution for both key and model. It performs **no network
calls and no LLM/diff reads** - purely local introspection. It mirrors `goose --version` +
`goose configure` (version/paths) but as a non-interactive status dump.

### Resolution rules to surface (the precedence that already exists in the code)

These are the live rules the command must report faithfully (do not re-implement; report
the same precedence the runtime applies):

- **API key** (`src/provider/mod.rs:167` `key_env_var`, `src/config.rs:237` `env_plan`):
  the env bridge only sets a key env var when it is **not already set**, so the effective
  precedence is **env var > inline config key**. Attribution must therefore be:
  1. env var set & non-blank -> `env var <NAME>`
  2. else provider present in config with `key = Some(..)` -> `config file`
  3. else -> `not set`
  Ollama is key-free: report its **endpoint + endpoint source** instead of a key
  (`GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` (normalized) > config `endpoint` > default
  `http://localhost:11434`).
- **Model** (`src/provider/mod.rs:271-293` `resolve_model`/`pick_model`): precedence
  `--model` flag > per-provider env var (first non-blank in order) > built-in default.
  Empty/whitespace flag and env values are skipped. Report the resolved value **and** which
  layer produced it (`flag` / `env var <NAME>` / `default`).
- **Config path** (`src/config.rs:76-92` `config_path`/`config_path_from`): `GCM_CONFIG`
  (non-empty) `/config.toml` > OS config dir (`directories` crate) `/config.toml`. Report
  the resolved path, whether the file exists, and which source produced the directory.

### Key code touchpoints

- `src/cli.rs:104` `Commands` enum (currently only `Config`); `VERSION` at `:7`.
- `src/main.rs:35` `run()`; `Commands::Config` dispatch at `:38-40` (the pattern to copy);
  `--json` flag honored at `:45`. Top-level flags `--model` (`cli.rs:90`) and `--json`
  (`cli.rs:63`) are parsed before the subcommand.
- `src/config.rs`: `Config`/`ProviderConfig` structs (`:42`/`:55`), `config_path`,
  `config_path_from`, `load`, `load_from`. Note: `apply_to_env` (`:226`) is the env bridge -
  **status must NOT call it** (it would copy inline config keys into env and corrupt
  attribution).
- `src/provider/mod.rs`: `ProviderId` (`:153`), `key_env_var` (pub, `:167`), `default_model`
  (private, `:178`), `model_env_vars` (private, `:193`), `resolve_model`/`pick_model`
  (private, `:271`). The status command needs read access to model resolution - expose a
  thin introspection helper here rather than duplicating the precedence (see Decomposition).
- `src/output.rs`: `Envelope`/`emit` (`:29`/`:172`), `SCHEMA_VERSION = 1` (`:13`). The
  existing `Envelope.status` is a fixed enum of commit outcomes (plan/noop/committed/
  fallback/error) and does **not** fit a status dump.

### Design decisions (resolving the open question and the JSON shape)

- **Models per provider**: the Linear "max 3 models" idea is resolved to **option (a)** -
  show the single effective model + its source. gcm resolves exactly one model per provider;
  a candidate list is out of scope.
- **JSON shape**: do **not** overload the commit `Envelope`. Add a dedicated `Serialize`
  struct (a `StatusReport`) that follows the same convention (`v: SCHEMA_VERSION`, then a
  status-shaped payload) and emit it via its own one-line `println!`-to-stdout helper,
  exactly as `output::emit` does. This keeps the commit envelope's `status` enum clean while
  matching the "versioned `v`, output-module pattern" requirement.
- **Provider naming in output**: use the canonical lowercase token (the `--provider` /
  `GCM_PROVIDER` value, matching `serde` lowercase, e.g. `groq`, `google`, `openai`,
  `anthropic`, `ollama`) as the stable JSON `name`. The human view may also show a friendly
  label, but JSON keys/values use the canonical token.

## 2. Acceptance Criteria

- [ ] **AC-1**: `gcm status` exits 0 without reading the working-tree diff and without any
  network/LLM call (works outside a git repo too - it does not touch `Repo`).
- [ ] **AC-2**: Human output shows gcm version (`cli::VERSION`), resolved config dir, config
  file path, whether the file exists, and the config-dir source (`GCM_CONFIG` vs OS default).
- [ ] **AC-3**: For each `ProviderId` (in canonical `cloud_then_ollama()` order: Groq,
  Google, OpenAI, Anthropic, Ollama), output shows: canonical name, whether it is the
  **effective selected** provider (see AC-7), whether it is activated (see AC-7), key source
  (`config file` / `env var <NAME>` / `not set`; Ollama: endpoint + endpoint source instead),
  and resolved model + source (`default` / `env var <NAME>` / `flag`). The `--model` flag is
  reported as the model source for **only** the selected provider (other providers ignore it).
- [ ] **AC-4**: No raw API key value is ever printed in stdout or JSON - only a `not set` /
  `set (env <NAME>)` / `set (config)` indicator. **No masked suffix** (no `…<last4>`): a
  high-entropy key tail can leak key-space and trip secret scanners, and the source label
  already provides provenance (review: Gemini P2, synthesis Disagreement #1). Verified by a
  test that sets a key env var to a known secret and asserts the secret substring never
  appears in stdout/JSON.
- [ ] **AC-5**: Both `gcm status --json` and `gcm --json status` emit exactly one valid JSON
  object on stdout with `v: 1` and a status payload; it round-trips through `jq`. All
  human/diagnostic text stays on stderr (stdout is pure JSON). Requires `--json` be
  `#[arg(global = true)]` so it parses after the subcommand.
- [ ] **AC-6**: Works with **no config file present** (shows env-derived state + a clear "no
  config file" / `exists: false` indicator) **and** with a config file present that mixes an
  inline-key provider and an env-key provider - attributing each correctly.
- [ ] **AC-7**: Activation + default semantics are concrete and live in tested pure
  functions:
  - **Cloud provider** (Groq/Google/OpenAI/Anthropic) is "activated" iff it is listed in the
    loaded config's `providers` **or** its key env var is set & non-blank.
  - **Ollama** is "activated" iff it is listed in config `providers` **or** `OLLAMA_HOST` /
    `GCM_OLLAMA_BASE_URL` is set & non-blank. It is **not** "active by default" on a vanilla
    machine with no config and no Ollama env (review: Gemini action 1, synthesis Agreement #3).
    Ollama always reports endpoint status instead of a key field, regardless of activation.
  - **Effective selected provider** (the "default" shown per AC-3): precedence `--provider`
    flag > `GCM_PROVIDER` env > `config.default` (usable config) > built-in `Groq`. Status
    computes this itself (it does not call `apply_to_env`, so it must add the `config.default`
    fallback that the env bridge would normally supply). An invalid/unknown `GCM_PROVIDER` is
    reported as an error (per AC-10) and the selection falls back to `Groq` for display.
- [ ] **AC-8**: `gcm status --help` lists the subcommand; `Cli::command().debug_assert()`
  still passes (no clap conflicts).
- [ ] **AC-9**: Exit code: `gcm status` exits **0** whenever it produces a report - including
  the misconfigured cases it is meant to surface (an unknown `GCM_PROVIDER` value, an
  unusable/malformed config, or an unresolvable config dir), which are reported as fields/notes
  rather than failures. A non-zero exit is reserved for a catastrophic internal error
  (e.g. serialization failure). Rationale: status is read-only introspection whose job is to
  *report* misconfiguration, not fail on it; automation reads the JSON fields, not the exit
  code. (Note: Ollama review suggested exit 1 on provider-selection failure - resolved to
  exit 0 + reported error field; flagged for owner at checkpoint.)
- [ ] **AC-10**: Robustness - `gcm status` never panics and never prompts: it must not invoke
  the onboarding wizard, must not return `OnboardingRequired`, and must handle
  `config_path() == None` and an invalid `GCM_PROVIDER` by reporting them, not aborting.

**Verification method**: `cargo test` (unit + acceptance), plus manual runs in Evaluation
table 5 below piping `gcm status --json | jq` and grepping stdout for an injected secret.

## 3. Constraints

**Must**:
- Dispatch `Status` at the **top of `run()`**, before `execute()` and therefore before
  `ensure_configured()` / `config::apply_to_env` / the onboarding wizard / `Repo` discovery -
  mirroring the `Commands::Config` interception at `main.rs:38-40` (review: synthesis Agreement
  #1/#2, both reviewers P1). This guarantees status never reads a diff, never makes a provider/
  LLM call, never prompts, and never returns `OnboardingRequired`.
- Report the **same precedence** the runtime uses (env > inline-config for keys; flag > env
  > default for models). Inspect `config::load()` and the raw environment **separately**;
  never call `config::apply_to_env` in the status path.
- **`--model` flag is scoped to the selected provider only.** When reporting model source,
  pass `args.model` to `resolve_model_with_source` **only** for the effective selected provider;
  all other providers resolve from env/default. Otherwise every provider would falsely report
  the flag value (owner review, blind spot 1).
- **Effective selected provider precedence** = `--provider` flag > `GCM_PROVIDER` env >
  `config.default` (when a usable config loaded) > built-in `Groq`. Status must replicate this
  **without** `apply_to_env` - so it explicitly falls back to `config.default` (which normally
  reaches `pick_provider_id` only via the env bridge). `pick_provider_id` alone is insufficient
  here because it stops at `GCM_PROVIDER > Groq` (`provider/mod.rs:245`) (owner review, blind
  spot 3). An invalid/unknown `GCM_PROVIDER` is **reported as an error field**, not silently
  coerced to Groq (AC-10).
- **`--json` must be a global flag** (`#[arg(global = true)]` in `cli.rs`) so `gcm status --json`
  parses (flags otherwise may not follow a unit subcommand). Both `gcm status --json` and
  `gcm --json status` must work (AC-5). This is backward-compatible (only widens where `--json`
  is accepted). `--provider`/`--model` stay top-level-only (must precede the subcommand).
- Mask secrets: never emit a raw key value in stdout or JSON (AC-4).
- Keep all introspection pure/local: no `Repo`, no `provider::select` construction that would
  require a key, no HTTP (no Ollama probe - report the configured endpoint, do not call it).
- `--json` output is a single valid JSON object on stdout; diagnostics to stderr.
- Reuse existing source-of-truth helpers (`ProviderId::key_env_var`, `config::config_path`,
  model env-var names) rather than hardcoding env var name strings a second time.
- Never panic: handle `config_path() == None` (no OS config dir) and an invalid/unknown
  `GCM_PROVIDER` by reporting them in the output, not by aborting (review: Gemini blind spots,
  synthesis Novel #2/#4).
- Report Ollama zero-egress status: a resolved Ollama model ending in `:cloud` proxies
  off-machine and is NOT zero-egress - surface this as a `zero_egress` boolean (JSON) and a
  note (human), reusing the existing `model.ends_with(":cloud")` check from
  `provider::select` (review: Gemini blind spot, synthesis Novel #1).

**JSON forward-compatibility**:
- Document (in the `StatusReport` doc comment and README/help) that JSON consumers must ignore
  unknown fields so the `v: 1` payload can gain fields without a breaking bump (review: Ollama
  P3, synthesis Novel #7).

**Must-not**:
- Must not add a network dependency or probe any endpoint.
- Must not overload the commit `Envelope.status` enum with a `status` variant.
- Must not call `apply_to_env` (corrupts key attribution) or mutate any env var.
- Must not print full API keys, nor any masked suffix derived from a key (AC-4).

**Prefer**:
- Pure functions for attribution (key source, model source, activation, config-dir source)
  taking explicit inputs (config, env-lookup closure) so they are unit-testable without
  touching process env - mirror the `config_path_from` / `env_plan(is_set)` style already in
  the codebase.
- A dedicated `status` module (e.g. `src/status.rs`) or a `status`-focused section of
  `output.rs` for the report struct + rendering, keeping `main.rs` dispatch thin.
- Snapshot-free assertions (substring/field checks) consistent with existing acceptance
  tests in `tests/onboarding.rs`; tests use a hermetic env (`GCM_CONFIG` override + cleared
  provider env vars), matching that file's pattern.
- Surface an insecure (non-0600) config file: `config::load` already warns to stderr; status
  may additionally report it as a field/note (review: Ollama P3, synthesis Novel #8). Optional,
  not required.

**Escalate when**:
- Exposing model/provider resolution requires a wider `pub` surface than the planned helpers
  (`resolve_model_with_source`, `pub(crate) pick_provider_id`, `pub(crate)` accessors) - confirm
  the minimal API first.
- Making `--json` global surfaces any interaction with the commit-flow flag parsing that
  changes existing behavior beyond "accepted in more positions" - stop and confirm.

## 4. Decomposition

1. **CLI surface**: add `Status` variant to `Commands` in `src/cli.rs` (doc comment:
   "Print active providers, models, paths, and config sources, then exit."). Mark `--json`
   `#[arg(global = true)]` so `gcm status --json` parses. Add a parse test mirroring
   `config_subcommand_parses` (`status_subcommand_parses`), plus a test that `gcm status --json`
   parses with `cli.json == true`. - files: `src/cli.rs`
2a. **Expose model accessors** (trivial): make `default_model` and `model_env_vars` `pub`
   (or add thin `pub` wrappers) in `src/provider/mod.rs` so the status layer can read them
   without duplicating the per-provider tables. No behavior change. - files: `src/provider/mod.rs`
2b. **Model-resolution introspection**: add a `pub` helper returning the resolved model
   **and** its source for a `ProviderId` given an optional `--model` flag - e.g.
   `pub fn resolve_model_with_source(id, cli_model) -> (String, ModelSource)` where
   `ModelSource` is `Flag | Env(&'static str) | Default` (the `Env` carries the winning var
   name, so Google's `GCM_GEMINI_MODEL` > `GCM_GOOGLE_MODEL` precedence is reportable).
   Refactor `resolve_model`/`pick_model` to delegate (no behavior change). Unit-test each
   precedence branch incl. the Google dual-env case. - files: `src/provider/mod.rs`
   (Split from a single sub-task per review: Ollama P2, synthesis Disagreement #2 - the
   accessor exposure + the resolver+refactor are separately estimable.)
2c. **Expose provider selection**: make `pick_provider_id` `pub(crate)` in
   `src/provider/mod.rs` so status can compute the effective selection, and add (or reuse) a
   helper for the `config.default` fallback. No behavior change. - files: `src/provider/mod.rs`
3. **Attribution + report model**: build the pure introspection layer (new `src/status.rs`):
   - `config_dir_source` (GCM_CONFIG vs default) and path/exists resolution from
     `config::config_path` + a file-exists check; handle `None` (no config dir) gracefully.
   - `selected_provider(cli_provider, config, env_lookup) -> Result<ProviderId, ..>` applying
     flag > `GCM_PROVIDER` > `config.default` > `Groq`; an invalid `GCM_PROVIDER` yields a
     reportable error (not a panic), display falls back to `Groq`.
   - `key_source(id, config, env_lookup) -> KeySource` (`EnvVar(name) | Config | NotSet`),
     applying env>config precedence with **non-blank** env checks; cloud only.
   - `ollama_endpoint_source(config, env_lookup) -> (endpoint, source)` with the full chain
     `GCM_OLLAMA_BASE_URL` > `OLLAMA_HOST` (via `ollama::normalize_host`, made `pub(crate)`) >
     config `endpoint` > default `http://localhost:11434`.
   - `activation(id, config, env_lookup) -> bool` per AC-7 (non-blank checks; Ollama only
     active via config or `OLLAMA_HOST`/`GCM_OLLAMA_BASE_URL`).
   - model source via `resolve_model_with_source`, passing `cli_model` **only** for the
     selected provider.
   - A `StatusReport` data struct (version, paths block, `Vec<ProviderStatus>`, optional
     `provider_error` for an invalid `GCM_PROVIDER`), built from the above. Secrets are never
     stored raw: only the source enum is kept (no masked suffix). - files: `src/status.rs`,
     (read) `src/config.rs`, `src/provider/mod.rs`, `src/provider/ollama.rs`
4. **JSON output**: add a `Serialize` shape for `StatusReport` (`v: SCHEMA_VERSION` +
   payload) and an `emit_status`-style stdout writer following `output::emit`. Decide whether
   it lives in `output.rs` or `status.rs` (prefer `status.rs` owning its render, reusing
   `output::SCHEMA_VERSION`). - files: `src/status.rs` (+ maybe `src/output.rs`)
5. **Human rendering + dispatch**: render the default human view grouped as Version / Paths /
   Providers; add `mod status;` and wire `Commands::Status` dispatch at the **top** of
   `src/main.rs::run()` (immediately after the `Commands::Config` arm, before `execute()` /
   `ensure_configured`), honoring `args.json` (JSON) vs human, and passing `args.provider` /
   `args.model` into the report builder. - files: `src/main.rs`, `src/status.rs`
6. **Tests**: unit tests for every pure attribution function (config/env/default key source,
   model source incl. Google dual-env, selected-provider precedence incl. `config.default`
   fallback, activation, config-dir source, masking) + an acceptance test (new `tests/status.rs`)
   driving the binary via `Command::new(env!("CARGO_BIN_EXE_gcm"))` (the `tests/onboarding.rs`
   pattern - **no `assert_cmd`/`cargo_bin`**, which is not a dependency) with `GCM_CONFIG` and a
   cleared provider env, covering: no-config-file case, mixed inline/env-key config,
   secret-never-printed (AC-4), `--json` validity/round-trip (both flag positions),
   invalid `GCM_PROVIDER`, malformed config, and Ollama endpoint source. - files: `src/cli.rs`,
   `src/provider/mod.rs`, `src/status.rs`, `tests/status.rs`

**Dependency order**: 1, 2a, 2b, 2c are independent and can go first. 3 depends on 2a/2b/2c
(model source, accessors, selection) and reads config/provider. 4 depends on 3 (the report
struct). 5 depends on 3+4. 6 is written alongside each unit (TDD) and finalized after 5.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `gcm status` outside any git repo, no config, clean env | exit 0; shows version, "no config file"/`exists:false`, every provider `not set` + default model `default` | `cd /tmp && GCM_CONFIG=$(mktemp -d) gcm status; echo $?` |
| 2 | Env-key + env-model attribution | groq key source = `env var GROQ_API_KEY`; model source = `env var GCM_GROQ_MODEL`, value `m-x` | `GCM_CONFIG=$(mktemp -d) GROQ_API_KEY=sk-secret123 GCM_GROQ_MODEL=m-x gcm status` |
| 3 | Inline-config key attribution | provider with inline key shows key source = `config file`; provider relying on env shows `env var <NAME>` | write a 0600 `config.toml` (one provider `key="sk-inline"`, one env-only) under `GCM_CONFIG`, run `gcm status` |
| 4 | Secret never printed (AC-4) | `sk-secret123` does **not** appear in stdout or `--json` output | `GROQ_API_KEY=sk-secret123 gcm status \| grep -c sk-secret123` -> `0`; same for `gcm status --json` |
| 5 | JSON validity + round-trip (AC-5) | one valid JSON object, `v == 1`, has providers array; jq exits 0 | `gcm status --json \| jq -e '.v==1 and (.providers\|length>=5)'` |
| 6 | Ollama endpoint source | Ollama shows endpoint + source (`OLLAMA_HOST`/`GCM_OLLAMA_BASE_URL`/`default`), never a key field | `OLLAMA_HOST=remote:11434 gcm status` |
| 7 | Model flag scoped to selected provider | `--provider openai --model foo` -> openai model `foo (flag)`; other providers keep their env/default model | `gcm --provider openai --model foo status` |
| 8 | Help + clap validity (AC-8) | `gcm status` listed in help; debug_assert passes; `gcm status --json` parses | `gcm --help \| grep status`; `gcm status --json \| jq .v`; `cargo test cli_definition_is_valid` |
| 9 | Invalid `GCM_PROVIDER` reported, exit 0 (AC-9/AC-10) | exit 0; output reports unknown provider as an error field, does not crash, falls back to Groq as selected for display | `GCM_PROVIDER=bogus gcm status --json \| jq '.provider_error'`; `echo $?` -> 0 |
| 10 | Malformed config falls back (edge) | exit 0; env-derived state shown, config reported unusable (stderr warning ok), stdout valid | `printf 'bad toml [' > $GCM_CONFIG/config.toml && gcm status --json \| jq .v` |
| 11 | `config.default` drives selection without env | config `default="openai"`, no flag/env -> openai is the selected/`[default]` provider | write 0600 config (default openai), `gcm status` |
| 12 | Google dual-env precedence | `GCM_GEMINI_MODEL` wins over `GCM_GOOGLE_MODEL`; source names `GCM_GEMINI_MODEL` | `GCM_GEMINI_MODEL=a GCM_GOOGLE_MODEL=b gcm status` |

**Edge cases to verify**:
- Blank/whitespace env values (`GROQ_API_KEY=""`, `GCM_GROQ_MODEL="   "`, `GCM_CONFIG=""`) are
  treated as unset for both attribution and activation (matches `pick_model`/`config_path_from`
  /`env_nonblank` behavior). A blank key env var must NOT mark a provider activated.
- Google's dual model env vars: `GCM_GEMINI_MODEL` takes precedence over `GCM_GOOGLE_MODEL`;
  attribution names the one that actually won (test #12).
- Config file present but malformed/wrong-version (`config::load()` returns `None`): status
  must still run, reporting env-derived state and that the config was not usable (do not
  abort; the existing `load` warning to stderr is acceptable) (test #10).
- A provider listed in config as env-only (`key = None`) but with no env var set: activated
  per config membership (AC-7) yet key source `not set` - both facts shown without conflict.
- Ollama on a clean machine (no config, no `OLLAMA_HOST`/`GCM_OLLAMA_BASE_URL`): **not**
  activated; still reports the default endpoint `http://localhost:11434` (source `default`),
  never a key field.
- Ollama `:cloud` model (e.g. `GCM_OLLAMA_MODEL=deepseek-v4-flash:cloud`): `zero_egress=false`
  in JSON and a note in human output; a local model -> `zero_egress=true`.
- `config_path()` returns `None` (no OS config dir): status reports "no config dir" gracefully,
  exit 0, no panic.
- `--json` with a non-blank `GCM_CONFIG` pointing at a dir with a 0644 (insecure) config:
  `load` ignores it with a stderr warning; stdout stays valid JSON.
