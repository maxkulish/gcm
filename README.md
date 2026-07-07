# gcm

`gcm` turns your working-tree changes into a clean, GPG/SSH-signed git commit with an
AI-generated [Conventional Commits](https://www.conventionalcommits.org/) message.

`gcm` reads your working-tree diff safely, asks an LLM provider to split it into
logical commit groups (a typed JSON plan via structured outputs), shows you the groups,
and commits the **first** group with its own message. Run it again to commit the next
group - a mixed change set becomes a series of clean, atomic commits. `--all` skips
grouping and commits everything as one. Providers are selectable by flag/env -
**Groq** (default), **Google (Gemini)**, **OpenAI**, **Anthropic**, and **Ollama** (local, no key) -
each via direct HTTP per its verified capability. Architecture is fixed by
[ADR-001](docs/adrs/001-foundational-architecture-decisions.md).

Beyond commits, **`gcm resolve`** uses the same providers to resolve an in-progress
merge, rebase, or cherry-pick: it feeds each conflict's three-way context
(`base` / `ours` / `theirs`) to the LLM, pre-merges the easy hunks with
[`mergiraf`](https://crates.io/crates/mergiraf) when available, validates the result,
and previews each file before writing - it never stages files or continues the merge on
its own. See [Resolving merge conflicts](#resolving-merge-conflicts-gcm-resolve).

## Privacy / data egress

`gcm` sends your **working-tree diff** and the **content of untracked, non-gitignored
files** to the configured LLM provider (Groq by default; Google, OpenAI, Anthropic, or Ollama when selected)
to generate the grouping plan and commit messages. `gcm resolve` additionally sends the
conflicting hunks and their surrounding file context to the provider. Gitignored files
(for example `.env`) are gathered with `git --exclude-standard` and are **never sent**. Review the selected
provider's data policy before use. This disclosure is also printed by `gcm --help`.

Add repo-local `.gcmignore` or `gcmignore` patterns to exclude additional paths from
analysis and commits. The ignore files themselves are excluded from provider prompts.
For best-effort credential scanning before provider egress, use
`--secret-scan=redact` to replace detected values or `--secret-scan=abort` to stop
before any provider request. The default is `off`.

**Zero-egress option:** `--provider=ollama` talks to a local Ollama daemon (default
`http://localhost:11434`), so with a local model **nothing leaves the machine** - the
privacy anchor. One caveat: an Ollama `*:cloud` model (e.g. `deepseek-v4-flash:cloud`)
is proxied by the daemon to Ollama Cloud and is therefore **not** zero-egress.

## Requirements

- Rust 1.75+ (build) / a `git` binary on `PATH` (runtime)
- An API key for the selected cloud provider: `GROQ_API_KEY` (default), `GEMINI_API_KEY`,
  `OPENAI_API_KEY`, or `ANTHROPIC_API_KEY` - **or** a running local
  [Ollama](https://ollama.com) daemon with a pulled model (`--provider=ollama`, no key)
- git commit signing configured (`commit.gpgsign=true` with a usable GPG or SSH key);
  every commit is signed (`git commit -S`)
- optional: [`mergiraf`](https://crates.io/crates/mergiraf) on `PATH` for `gcm resolve`'s
  structural pre-resolution stage (structurally trivial conflicts are merged with no LLM
  call); gcm falls back to a pure-LLM path when it is absent

## Install

### Option A - Homebrew (recommended)

```sh
brew install maxkulish/homebrew-tap/gcm
gcm --version                 # update later with: brew upgrade gcm
```

### Option B - prebuilt release binary

Download the archive for your platform from the [latest release](https://github.com/maxkulish/gcm/releases/latest).
Prebuilt targets: macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`) and Linux
(`aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl`; the Linux builds are static
and run on any distro).

```sh
# pick the archive for your platform, e.g. Apple Silicon macOS:
VER=v0.2.0
TARGET=aarch64-apple-darwin
curl -LO "https://github.com/maxkulish/gcm/releases/download/$VER/gcm-$VER-$TARGET.tar.gz"
curl -LO "https://github.com/maxkulish/gcm/releases/download/$VER/gcm-$VER-$TARGET.tar.gz.sha256"

# verify the checksum
shasum -a 256 -c "gcm-$VER-$TARGET.tar.gz.sha256"   # macOS
# sha256sum -c "gcm-$VER-$TARGET.tar.gz.sha256"      # Linux

tar xzf "gcm-$VER-$TARGET.tar.gz"                    # extracts: gcm, LICENSE
install -m 0755 gcm ~/.local/bin/gcm                 # put it on your PATH

# macOS only: clear the Gatekeeper quarantine on the unsigned binary
xattr -d com.apple.quarantine ~/.local/bin/gcm 2>/dev/null || true

gcm --version
```

### Option C - `cargo install` from source

```sh
cargo install --git https://github.com/maxkulish/gcm --locked
```

### Option D - build it yourself

```sh
cargo build --release
# the binary is ./target/release/gcm; copy it onto your PATH, e.g.:
install -m 0755 target/release/gcm ~/.local/bin/gcm
```

> Migrating from the bash `git-commit-ai.sh`? See the
> [cutover guide](docs/guides/cutover-from-bash.md) for the alias mapping and a
> one-line rollback.

## Quick start

Once `gcm` is on your `PATH`, go from zero to your first AI commit in three steps.

**1. Configure a provider.** The first run with no config launches an interactive setup
wizard automatically; you can also invoke it explicitly:

```sh
gcm config            # enable a provider, enter its API key, choose a default
```

Pick a cloud provider and paste its key (`GROQ_API_KEY`, `GEMINI_API_KEY`,
`OPENAI_API_KEY`, or `ANTHROPIC_API_KEY`), or choose **Ollama** for a fully local,
zero-egress setup (needs a running daemon, no key). Commits are always signed, so make
sure signing is configured once:

```sh
git config --global commit.gpgsign true   # with a usable GPG or SSH signing key
```

**2. Commit your changes.** In any repo with uncommitted work:

```sh
gcm                   # groups the changes, shows the plan, commits the first group on [Y]
gcm                   # run again to commit the next group
gcm --all             # or commit everything as one
gcm --dry-run         # preview only; stage or commit nothing
```

**3. Resolve merge conflicts.** When a merge, rebase, or cherry-pick leaves conflicts:

```sh
git merge feature-branch      # ...leaves conflicts
gcm resolve --dry-run         # preview the proposed resolutions, write nothing
gcm resolve                   # resolve each file, confirming [Y/n/e] before it writes
git add -A && git commit      # you stage and finish the merge - gcm never does it for you
```

Run `gcm status` anytime to see which provider, model, and key are active. See
[Resolving merge conflicts](#resolving-merge-conflicts-gcm-resolve) for the full resolver
behavior and configuration.

## Usage

```sh
export GROQ_API_KEY=...          # key for the selected provider
gcm                              # group changes, show the plan, confirm [Y/n/e], commit group 1
gcm                              # run again to commit the next group
gcm --all                        # skip grouping; commit everything as one
gcm --dry-run                    # preview the plan (or the --all message); stage/commit nothing
gcm --plan-only                  # same preview as --dry-run, but never touches the plan cache
                                 # and can run without an API key on the --all path

gcm --json                       # emit a machine-readable envelope on stdout; diagnostics on stderr
gcm --json --plan-only           # JSON plan preview; non-destructive
gcm --json --yes                 # unattended JSON commit
gcm --yes                        # auto-confirm (non-interactive / CI / agents); alias --no-input
gcm --provider=google            # use Gemini (also: --provider=openai, --provider=anthropic); default is groq
gcm --provider=openai --model=gpt-5.4-mini   # override the model for a provider
gcm --provider=anthropic         # use Anthropic (forced tool-use for structured output)
gcm --provider=ollama            # local, zero-egress (no key); needs a running Ollama daemon
gcm config                       # run the interactive provider setup wizard and exit
gcm provider                     # interactively pick a provider, fetch + enable its models, set a default
gcm status                       # show active providers, models, paths, and config sources (read-only)
gcm status --json                # the same as a machine-readable JSON object
gcm --reconfigure                # re-run the wizard (update keys/selection), then continue
gcm --secret-scan=redact         # redact common credential-looking values before provider egress
gcm --secret-scan=abort          # abort before provider egress when a credential-looking value is found
gcm resolve                      # resolve in-progress merge/rebase/cherry-pick conflicts (see below)
gcm resolve --dry-run            # preview the resolutions; write nothing
gcm --version                    # build-stamped version (crate version + git short SHA)
```

At the prompt: `Y`/Enter commits group 1, `n` aborts (exit 0, nothing staged), `e` opens
`$EDITOR` (default `vim`) to edit group 1's message first.

### First-run setup

On an unconfigured first run - no `config.toml`, no provider hint in the environment
(`--provider`, a non-blank `GCM_PROVIDER`, or any cloud key) - `gcm` launches an
interactive wizard. It lets you enable one or more providers, captures each cloud key
(reused from the environment when already exported, otherwise typed with echo disabled),
probes the Ollama daemon when selected, and picks a default. The result is written to
`config.toml` in your OS config dir with `0600` permissions.

Re-run setup anytime to rotate keys or change selections: `gcm config` (wizard then
exit) or `gcm --reconfigure` (wizard then continue with the commit flow). The wizard is
idempotent - it overwrites the existing file cleanly.

### Selecting which models to use (`gcm provider`)

`gcm provider` opens a richer, model-aware wizard (a polished `cliclack` interface).
Pick a provider, and gcm fetches that provider's available models live from its API
(falling back to a built-in list if there is no key or the fetch fails). Type to filter
the list, `space` to toggle the models you want to **enable**, `enter` to submit, then
choose one as the default. The selection is saved to `config.toml`, preserving every
other provider you have configured.

Once a provider has a non-empty enabled set, gcm enforces it: a `--model` (or per-provider
model env var, or config default) outside that set is rejected with a clear message, so
you cannot accidentally use a disabled or non-text model. Leaving the set empty (the
default, and how existing configs migrate) keeps models unrestricted - any model is
allowed, exactly as before. The cloud-provider key is read from the environment or your
existing config, and only prompted (masked, never echoed) when none is found; nothing is
written until you finish the wizard.

`gcm provider` needs an interactive terminal. The config-file format version is bumped to
2 to record the enabled-model set; existing v1 configs load unchanged.

### Inspecting the active configuration

`gcm status` answers "what will gcm do right now, and why" without any network call,
diff read, or LLM request. It prints the build version, the resolved config dir / file
path and where that path came from (`GCM_CONFIG` vs the OS default), and per-provider:
whether it is the effective selection, whether it is activated, the key source
(`config file` / `env var <NAME>` / `not set` - never the key itself), the resolved model
and its source (`flag` / `env var <NAME>` / `default`), and for Ollama the endpoint and
whether the model is zero-egress. Add `--json` for a versioned, scriptable object
(`v: 1`); JSON consumers should ignore unknown fields so the payload can grow without a
breaking version bump. The command always exits 0 - a misconfiguration (an unknown
`GCM_PROVIDER`, an unusable config) is reported as a field, not a failure.

Config is a fallback layer between the environment and the built-in default: precedence
is `--flag` > env var > `config.toml` > default, and a value already in the environment
is never overwritten. A cloud key you export is recorded by reference (the env var name),
not copied into the file; a key you type at the prompt is stored inline in the `0600`
file - so treat `config.toml` as a secret.

In a non-TTY context (CI, pipes) where setup would be required, `gcm` does **not** hang
on the wizard: it prints the `export` lines and a `config.toml` template to stderr and
exits non-zero (in `--json` mode, a `status: error`, `code: OnboardingRequired` envelope
on stdout, instructions on stderr). Export a key and set `GCM_PROVIDER`, or write the
config file, to proceed unattended.

### Machine-readable mode (`--json`)

When `--json` is set, `gcm` prints exactly one JSON object on stdout and sends all
logs/warnings to stderr. The envelope always contains `v: 1` and a `status` field:
`plan`, `noop`, `committed`, `fallback`, or `error`. The `mode` field is one of
`plan_only`, `dry_run`, `single`, or `grouped`. Use this for CI, agents, and scripts
that need a stable contract instead of parsing human prose.

Example:

```sh
gcm --json --plan-only | jq -e '.status == "plan" and .mode == "plan_only"'
```

### Providers

Select with `--provider` or `GCM_PROVIDER` (precedence: flag > env > default `groq`).
Override the model with `--model` or the per-provider env var.

| Provider | `--provider` | API key | Default model | Model env | Structured output |
|----------|--------------|---------|---------------|-----------|-------------------|
| Groq (default) | `groq` | `GROQ_API_KEY` | `openai/gpt-oss-120b` | `GCM_GROQ_MODEL` | strict `json_schema` |
| Google (Gemini) | `google` (alias `gemini`) | `GEMINI_API_KEY` | `gemini-3.1-flash-lite` | `GCM_GEMINI_MODEL` (or `GCM_GOOGLE_MODEL`) | `responseSchema` |
| OpenAI | `openai` | `OPENAI_API_KEY` | `gpt-5.4-mini` | `GCM_OPENAI_MODEL` | strict `json_schema` |
| Anthropic | `anthropic` | `ANTHROPIC_API_KEY` | `claude-haiku-4-5` | `GCM_ANTHROPIC_MODEL` | forced tool-use (`input_schema`) |
| Ollama (local) | `ollama` | none | `gemma4:e4b-mlx` | `GCM_OLLAMA_MODEL` | native `format` (model-dependent) |

Reasoning models emit no chain-of-thought into the plan or message (per-provider
suppression + a `<think>` backstop). OpenAI reasoning models (`o1`/`o3`-style) are
supported as `--model` overrides; the default `gpt-5.4-mini` is non-reasoning.

**Ollama (local, zero-egress):** needs a running daemon and a pulled model; no API key.
The endpoint is `http://localhost:11434` by default - override with `OLLAMA_HOST` (e.g.
`OLLAMA_HOST=host:port`, scheme/port auto-completed) or `GCM_OLLAMA_BASE_URL` (a full URL).
gcm uses the native `/api/chat` with a JSON-Schema `format`; structured-output fidelity
varies by model, so it falls back to defensive parsing + retry. If the daemon is not
running or the model is not pulled, gcm prints an actionable error (`ollama serve` /
`ollama pull <model>`). Local inference can be slow - raise `GCM_HTTP_TIMEOUT_SECS` for
large diffs. A `*:cloud` model is proxied to Ollama Cloud and is **not** zero-egress.

### Configuration (environment)

| Variable | Default | Purpose |
|----------|---------|---------|
| `GROQ_API_KEY` / `GEMINI_API_KEY` / `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` | (one required for cloud) | API key for the selected cloud provider; Ollama needs none |
| `GCM_PROVIDER` | `groq` | Provider: `groq`, `google`, `openai`, `anthropic`, or `ollama` (flag `--provider` wins) |
| `GCM_GROQ_MODEL` / `GCM_GEMINI_MODEL` / `GCM_OPENAI_MODEL` / `GCM_ANTHROPIC_MODEL` / `GCM_OLLAMA_MODEL` | per-provider default | Model id (flag `--model` wins) |
| `GCM_GROQ_BASE_URL` / `GCM_GEMINI_BASE_URL` / `GCM_OPENAI_BASE_URL` / `GCM_ANTHROPIC_BASE_URL` / `GCM_OLLAMA_BASE_URL` | per-provider default | Override the API base URL |
| `OLLAMA_HOST` | `localhost:11434` | Ollama daemon host (scheme/port auto-completed); `GCM_OLLAMA_BASE_URL` takes precedence |
| `GCM_DIFF_TOTAL_BYTES` / `GCM_DIFF_PER_FILE_BYTES` | per-provider | Override the diff budget |
| `GCM_SECRET_SCAN` | `off` | Optional pre-send scan: `off`, `redact`, or `abort` (flag `--secret-scan` wins) |
| `EDITOR` | `vim` | Editor for the `e` (edit) option |
| `GCM_DEBUG` | (unset) | Legacy shortcut: when set to a non-empty, non-`0` value it enables debug-level logging (overridden by `GCM_LOG_LEVEL`) |
| `GCM_LOG_LEVEL` | `off` | Logging level: `off`, `error`, `warn`, `info`, `debug`, `trace`. Precedence over `GCM_DEBUG`; all logs go to stderr |
| `GCM_RETRY_MAX` | `3` | Max retries for transient (429/5xx) failures |
| `GCM_RETRY_BASE_MS` | `500` | Base backoff in ms (doubles per attempt) |
| `GCM_RETRY_MAX_MS` | `8000` | Per-attempt backoff cap in ms |
| `GCM_HTTP_TIMEOUT_SECS` | `60` | Per-request client timeout (raise for slow reasoning models) |
| `GCM_CONFIG` | OS config dir | Directory holding `config.toml` (overrides the default location; useful for tests/relocation) |

The persisted config lives in `config.toml` (TOML, `0600`) inside the OS config dir -
`~/.config/gcm/config.toml` (Linux, honoring `XDG_CONFIG_HOME`) or
`~/Library/Application Support/gcm/config.toml` (macOS) - or under `$GCM_CONFIG` when set.
It records the enabled providers, the default, optional inline keys, and the Ollama
endpoint. See [First-run setup](#first-run-setup).

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success, or user aborted at the prompt, or no changes to commit |
| 1 | Runtime error (not a repo, missing key, HTTP failure, signing/commit failure, onboarding required in a non-TTY) |
| 2 | CLI usage error |

In a non-interactive context (no TTY) without `--yes`/`--no-input`, `gcm` exits non-zero
with an actionable message rather than hanging on a prompt.

## Resolving merge conflicts (`gcm resolve`)

`gcm resolve` resolves the conflicts left by an in-progress `git merge`, `git rebase`, or
`git cherry-pick`. It reuses the same provider layer as the commit flow, but instead of
writing a commit message it proposes a resolution for each conflict and lets you review it
before anything touches your files.

The pipeline is layered so the LLM only sees what genuinely needs judgement:

1. **Normalize** - each conflicted file is re-read as a three-way (`zdiff3`) conflict, so
   `base`, `ours`, and `theirs` are all available.
2. **Structural pre-merge** - if [`mergiraf`](https://crates.io/crates/mergiraf) is on
   `PATH`, structurally trivial hunks are merged with no LLM call. Skipped when it is
   absent or with `--no-mergiraf`.
3. **LLM resolution** - the remaining hunks go to the provider with their three-way
   context, instructed to combine both sides' intent and only use symbols that already
   exist in the code.
4. **Validation** - the resolved file is checked. With a validation command
   (`--conflict-validate-cmd`, e.g. `cargo check`), a failure triggers one bounded retry
   and then escalates the file to you rather than writing a broken resolution.
5. **Preview** - each file is shown with a `[Y/n/e]` prompt (`e` opens `$EDITOR`). gcm
   writes the resolved file only on `Y`, and **never runs `git add` or `--continue`** -
   staging and finishing the merge stay in your hands.

```sh
gcm resolve                                       # resolve conflicts, confirming each file
gcm resolve --dry-run                             # preview resolutions; write nothing
gcm resolve --yes                                 # non-interactive: accept validated resolutions
gcm resolve --no-mergiraf                         # skip the structural pre-merge stage
gcm resolve --conflict-validate-cmd "cargo check" # gate each resolution on a build/test
gcm resolve --conflict-auto-policy complex        # send every hunk to the LLM (no auto-resolve)
gcm resolve --provider=ollama                     # local, zero-egress resolution
```

**Safety guarantees:** preview-before-write is always on; EOF / non-interactive input
never auto-accepts (pass `--yes` to opt in); a resolution that fails validation is
escalated rather than written; and an unparseable model response leaves the file
conflicted for you to handle by hand.

### `[conflict]` configuration

Defaults are conservative; tune them in the `[conflict]` table of `config.toml`, or with
the matching `--conflict-*` flags (which take precedence).

| Key / flag | Default | Purpose |
|------------|---------|---------|
| `temperature` / `--conflict-temperature` | `0.1` | Sampling temperature for resolutions - kept low for reproducibility |
| `auto_policy` / `--conflict-auto-policy` | `trivial` | Which hunks to auto-resolve: `trivial` (identical / one-side-only), `moderate` (reserved), or `complex` (auto-resolve nothing - send every hunk to the LLM) |
| `validate_cmd` / `--conflict-validate-cmd` | (none) | Shell command run against each resolved file; a failure retries once, then escalates |
| `sensitive_paths` / `--conflict-sensitive-paths` | (none) | Glob patterns whose files always require manual review |
| `mergiraf` / `--no-mergiraf` | `true` | Use `mergiraf` for structural pre-resolution when it is on `PATH` |

```toml
[conflict]
temperature = 0.1
auto_policy = "trivial"
validate_cmd = "cargo check"
sensitive_paths = ["migrations/*", "**/secrets.rs"]
mergiraf = true
```

`.gcmignore` and `--secret-scan` apply to conflict resolution exactly as they do to the
commit flow.

## Behavior notes

- **Grouping & progression**: each run requests a fresh plan over the *current* changes,
  commits group 1, and leaves the rest. Re-running re-analyses the remainder and commits
  the next group - no plan is cached (that lands in a later slice).
- **Whole-file staging**: grouping operates on whole files across the entire working tree,
  so it overrides any manual hunk-level (`git add -p`) staging. Group 1's files are staged
  in full; later groups are left unstaged - their changes stay in the working tree and are
  never lost. Use `--all` if you want everything in one commit.
- **Plan cache**: `gcm` persists the last grouping plan per repo so the next run can
  commit the next group without re-calling the LLM. Use `--reset` to discard the
  cached plan and re-analyze from scratch; in `--json` mode `--reset` clears the
  cache and then emits the normal noop/plan/committed envelope for the current
  tree (there is no separate `reset` status).
- **Safe fallback**: if the provider can't return a usable plan (structured output
  unavailable, unparseable JSON, or a plan that references files outside the change set),
  `gcm` announces it and falls back to a single commit. An unresolved merge conflict makes
  `gcm` stop with an error rather than risk committing conflict markers - run
  [`gcm resolve`](#resolving-merge-conflicts-gcm-resolve) to resolve them first.
- **Resilient provider calls**: failures are classified into typed errors (rate-limit,
  bad-request, auth, server, timeout, transport, parse). Transient ones (HTTP 429 and 5xx)
  are retried with bounded exponential backoff, so a rate limit or a server blip self-heals
  with no user-visible failure; 400 and auth errors fail fast with an actionable message and
  are never retried. All retries happen before anything is staged. Set `GCM_DEBUG=1` to see
  the typed error and retry attempts.
- **Transactional**: messages are generated before anything is staged. If you decline, or
  signing / a pre-commit hook fails, the index is restored to its pre-run state.
- **Safe diff read**: binary files are elided, each file's diff is truncated independently
  with a `[diff omitted: N bytes]` placeholder, untracked content is bounded by a
  file-count and byte cap (so an un-ignored directory of thousands of files cannot freeze
  the CLI), gitignored files are excluded, and `.gcmignore`/`gcmignore` patterns exclude
  additional paths from provider-bound analysis.
- **Path-safe**: file lists come from `git status --porcelain=v1 -uall -z` (NUL-delimited),
  and staging feeds exact paths to `git` literally, so names with spaces, `->`, unicode, or
  glob characters (`*`, `?`) are handled correctly.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
./scripts/acceptance.sh          # end-to-end checks against throwaway repos
```
