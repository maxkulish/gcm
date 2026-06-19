# gcm

`gcm` turns your working-tree changes into a clean, GPG/SSH-signed git commit with an
AI-generated [Conventional Commits](https://www.conventionalcommits.org/) message.

This is the first slice (the "tracer bullet", [CLO-486](https://linear.app/cloud-ai/issue/CLO-486)):
it reads the working-tree diff safely, asks [Groq](https://groq.com/) for one commit
message, confirms with you, and creates a single signed commit. Semantic grouping, the
plan cache, multiple providers, and retries arrive in later slices. Architecture is fixed
by [ADR-001](docs/adrs/001-foundational-architecture-decisions.md).

## Privacy / data egress

`gcm` sends your **working-tree diff** and the **content of untracked, non-gitignored
files** to the configured LLM provider (Groq) to generate the commit message. Gitignored
files (for example `.env`) are gathered with `git --exclude-standard` and are **never
sent**. Review [Groq's data policy](https://groq.com/privacy-policy/) before use. This
disclosure is also printed by `gcm --help`.

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
gcm                              # gather diff, generate message, confirm [Y/n/e], commit
gcm --dry-run                    # preview the message only; stage and commit nothing
gcm --yes                        # auto-confirm (non-interactive / CI / agents); alias --no-input
gcm --version                    # build-stamped version (crate version + git short SHA)
```

At the prompt: `Y`/Enter commits, `n` aborts (exit 0, nothing staged), `e` opens
`$EDITOR` (default `vim`) to edit the message first.

### Configuration (environment)

| Variable | Default | Purpose |
|----------|---------|---------|
| `GROQ_API_KEY` | (required) | Groq API key |
| `GCM_GROQ_MODEL` | `openai/gpt-oss-120b` | Groq model id |
| `GCM_GROQ_BASE_URL` | `https://api.groq.com/openai/v1` | Override the API base URL |
| `EDITOR` | `vim` | Editor for the `e` (edit) option |

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success, or user aborted at the prompt, or no changes to commit |
| 1 | Runtime error (not a repo, missing key, HTTP failure, signing/commit failure) |
| 2 | CLI usage error |

In a non-interactive context (no TTY) without `--yes`/`--no-input`, `gcm` exits non-zero
with an actionable message rather than hanging on a prompt.

## Behavior notes

- **Transactional**: the message is generated before anything is staged. If you decline,
  or signing / a pre-commit hook fails, the index is restored to its pre-run state.
- **Safe diff read**: binary files are elided, untracked content is bounded by a
  file-count and byte cap (so an un-ignored directory of thousands of files cannot freeze
  the CLI), and gitignored files are excluded.
- **Single commit**: this slice commits all changes as one commit; `--all` selects that
  behavior explicitly.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
./scripts/acceptance.sh          # end-to-end checks against throwaway repos
```
