# Cutover: from the bash `git-commit-ai.sh` to the Rust `gcm`

This guide repoints your shell aliases from the legacy bash script
(`/opt/script/git-commit-ai.sh`) to the Rust `gcm` binary. The cutover is
**reversible**: rollback is a one-line alias change, and the bash script is left
in place untouched.

## Before you start

1. Install `gcm` and confirm it's on your `PATH`:
   ```sh
   gcm --version
   ```
   (See [Install](../../README.md#install) for the prebuilt binary, `cargo install`,
   or build-from-source options.)
2. **Leave `/opt/script/git-commit-ai.sh` exactly where it is.** Do not delete or
   move it - it stays as your rollback target.

## Alias migration matrix

Exact mapping from the current shell aliases (all pointing at
`/opt/script/git-commit-ai.sh`) to the Rust invocation. Provider/model selection that
the bash script encoded with `--provider` and `GCM_GROQ_MODEL` is now plain `gcm`
flags (`--provider`, `--model`); precedence is `--flag` > env var > `config.toml` > default.

| Alias | Provider | Model | Old (bash) | New (Rust) |
|-------|----------|-------|------------|------------|
| `gcm` | Anthropic (personal default) | haiku | `git-commit-ai.sh` | `gcm` |
| `gcmq` | Groq | `openai/gpt-oss-120b` | `git-commit-ai.sh --provider=groq` | `gcm --provider=groq` |
| `gcmq20` | Groq | `openai/gpt-oss-20b` | `GCM_GROQ_MODEL=... git-commit-ai.sh --provider=groq` | `gcm --provider=groq --model=openai/gpt-oss-20b` |
| `gcmq27` | Groq | `qwen/qwen3.6-27b` | `GCM_GROQ_MODEL=... git-commit-ai.sh --provider=groq` | `gcm --provider=groq --model=qwen/qwen3.6-27b` |
| `gcmg` | Google | `gemini-3.1-flash-lite` | `git-commit-ai.sh --provider=google` | `gcm --provider=google` |
| `gcmo` (new) | OpenAI | `gpt-4o-mini-2024-07-18` (configurable) | n/a (new provider) | `gcm --provider=openai` |
| `gcml` (new) | Ollama (local) | user-pulled model | n/a (new provider) | `gcm --provider=ollama` |
| `gcmc` | ~~Cerebras~~ | n/a | `--provider=cerebras` (commented out) | **dropped** (not in v1) |
| `gcms` | none | n/a | `git commit -S -m` | **unchanged** (not part of `gcm`) |

Notes:
- **`gcm` (bare) default:** the shipped open-source default provider is **Groq**. As the
  primary user you keep **Anthropic Haiku** as your personal default by selecting it in
  the first-run wizard (`gcm config`) - the bare `gcm` alias then uses Anthropic without
  any flag.
- **`gcmo` / `gcml`** are new providers (OpenAI, local Ollama) that the bash script never
  had; add them if you want them.
- **`gcmc`** (Cerebras) is dropped entirely - omit it.
- **`gcms`** is a plain `git commit -S -m` alias, unrelated to `gcm` - leave it as is.

## Repoint your `~/.zshrc`

Replace the old block (the lines pointing at `/opt/script/git-commit-ai.sh`) with:

```sh
# gcm (Rust) - https://github.com/maxkulish/gcm
alias gcm="gcm"                                                   # Anthropic Haiku via `gcm config`
alias gcmq="gcm --provider=groq"                                  # Groq gpt-oss-120b
alias gcmq20="gcm --provider=groq --model=openai/gpt-oss-20b"     # Groq gpt-oss-20b
alias gcmq27="gcm --provider=groq --model=qwen/qwen3.6-27b"       # Groq Qwen3.6 27B
alias gcmg="gcm --provider=google"                                # Gemini 3.1 Flash Lite
alias gcmo="gcm --provider=openai"                                # OpenAI gpt-4o-mini (new)
alias gcml="gcm --provider=ollama"                                # local Ollama, zero-egress (new)
# gcmc (Cerebras) dropped; gcms unchanged (git commit -S -m)
```

> The `alias gcm="gcm"` line is harmless (an alias may share its command's name); it exists
> only so the whole block lives together and is trivial to comment out for rollback. You can
> omit it and just call the binary directly.

Reload your shell:

```sh
source ~/.zshrc
```

## Validate side-by-side

Before relying on the new tool, sanity-check it on a scratch repo:

```sh
gcm --plan-only        # preview grouping; touches nothing, no API key needed on the --all path
gcm --dry-run          # preview the message/plan; stages and commits nothing
```

Message text need not be byte-identical to the bash output (LLM output is
non-deterministic), but the **group structure** should look equivalent. The plan cache
is preserved across runs, so an in-flight grouping session survives the swap.

## Rollback (one line)

If anything misbehaves, revert in a single change - repoint the aliases back at the
untouched bash script. Either comment out the `gcm` block above and re-enable your old
block, or point the aliases back directly:

```sh
alias gcmq="/opt/script/git-commit-ai.sh --provider=groq"
# ...and the rest, exactly as before
source ~/.zshrc
```

Because `/opt/script/git-commit-ai.sh` was never touched, the bash tool works exactly as
it did before the cutover.
