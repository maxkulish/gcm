# gcm

`gcm` turns your working-tree changes into a clean, GPG/SSH-signed git commit with an
AI-generated [Conventional Commits](https://www.conventionalcommits.org/) message.

`gcm` reads your working-tree diff safely, asks [Groq](https://groq.com/) to split it into
logical commit groups (a typed JSON plan via structured outputs), shows you the groups,
and commits the **first** group with its own message. Run it again to commit the next
group - a mixed change set becomes a series of clean, atomic commits. `--all` skips
grouping and commits everything as one. The plan cache, multiple providers, and retries
arrive in later slices; architecture is fixed by
[ADR-001](docs/adrs/001-foundational-architecture-decisions.md).

## Privacy / data egress

`gcm` sends your **working-tree diff** and the **content of untracked, non-gitignored
files** to the configured LLM provider (Groq) to generate the grouping plan and commit
messages. Gitignored files (for example `.env`) are gathered with `git --exclude-standard`
and are **never sent**. Review [Groq's data policy](https://groq.com/privacy-policy/)
before use. This disclosure is also printed by `gcm --help`.

## Requirements

- Rust 1.75+ (build) / a `git` binary on `PATH` (runtime)
- A Groq API key in `GROQ_API_KEY`
- git commit signing configured (`commit.gpgsign=true` with a usable GPG or SSH key);
  every commit is signed (`git commit -S`)

## Install

```sh
cargo build --release
# the binary is ./target/release/gcm; copy it onto your PATH, e.g.:
install -m 0755 target/release/gcm ~/.local/bin/gcm
```

## Usage

```sh
export GROQ_API_KEY=...          # required
gcm                              # group changes, show the plan, confirm [Y/n/e], commit group 1
gcm                              # run again to commit the next group
gcm --all                        # skip grouping; commit everything as one
gcm --dry-run                    # preview the plan (or the --all message); stage/commit nothing
gcm --yes                        # auto-confirm (non-interactive / CI / agents); alias --no-input
gcm --version                    # build-stamped version (crate version + git short SHA)
```

At the prompt: `Y`/Enter commits group 1, `n` aborts (exit 0, nothing staged), `e` opens
`$EDITOR` (default `vim`) to edit group 1's message first.

### Configuration (environment)

| Variable | Default | Purpose |
|----------|---------|---------|
| `GROQ_API_KEY` | (required) | Groq API key |
| `GCM_GROQ_MODEL` | `openai/gpt-oss-120b` | Groq model id |
| `GCM_GROQ_BASE_URL` | `https://api.groq.com/openai/v1` | Override the API base URL |
| `EDITOR` | `vim` | Editor for the `e` (edit) option |
| `GCM_DEBUG` | (unset) | When set (not `0`), print the typed provider error and each retry attempt to stderr |
| `GCM_RETRY_MAX` | `3` | Max retries for transient (429/5xx) failures |
| `GCM_RETRY_BASE_MS` | `500` | Base backoff in ms (doubles per attempt) |
| `GCM_RETRY_MAX_MS` | `8000` | Per-attempt backoff cap in ms |

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success, or user aborted at the prompt, or no changes to commit |
| 1 | Runtime error (not a repo, missing key, HTTP failure, signing/commit failure) |
| 2 | CLI usage error |

In a non-interactive context (no TTY) without `--yes`/`--no-input`, `gcm` exits non-zero
with an actionable message rather than hanging on a prompt.

## Behavior notes

- **Grouping & progression**: each run requests a fresh plan over the *current* changes,
  commits group 1, and leaves the rest. Re-running re-analyses the remainder and commits
  the next group - no plan is cached (that lands in a later slice).
- **Whole-file staging**: grouping operates on whole files across the entire working tree,
  so it overrides any manual hunk-level (`git add -p`) staging. Group 1's files are staged
  in full; later groups are left unstaged - their changes stay in the working tree and are
  never lost. Use `--all` if you want everything in one commit.
- **Safe fallback**: if the provider can't return a usable plan (structured output
  unavailable, unparseable JSON, or a plan that references files outside the change set),
  `gcm` announces it and falls back to a single commit. An unresolved merge conflict makes
  `gcm` stop with an error rather than risk committing conflict markers.
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
  the CLI), and gitignored files are excluded.
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
