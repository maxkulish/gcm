# Mods to Crush Migration - Decision Document

## Context

Charmbracelet sunset `mods` on March 9, 2026 and archived the repository. The successor is `crush`, their agentic coding CLI. The `mods` binary still works (no server dependency), but receives no updates, bug fixes, or security patches.

The `gcm` script (`git-commit-ai.sh`) uses `mods` for three of four providers (Groq, Cerebras, Google). The fourth provider (Claude Haiku) uses the `claude` CLI directly.

## Current mods Usage in gcm

The script calls `mods` in a narrow, predictable pattern:

```bash
echo "$PROMPT" | mods -a <provider> -m <model> -r --no-cache -q 2>/dev/null
```

Flags used:
- `-a <provider>` - select API endpoint (groq, cerebras, google)
- `-m <model>` - select model
- `-r` - raw output (no syntax highlighting)
- `--no-cache` - don't save conversation
- `-q` - quiet (suppress UI chrome)
- `2>/dev/null` - suppress errors (silent fallback)

Configuration: `~/Library/Application Support/mods/mods.yml` (YAML, manually maintained)

## Crush Equivalent

`crush run` provides the non-interactive replacement:

```bash
echo "$PROMPT" | crush run --model <provider>/<model> --quiet
```

Key differences:

| Aspect | mods | crush |
|--------|------|-------|
| Model selection | `-a provider -m model` (separate flags) | `--model provider/model` (single flag) |
| Raw output | `-r` flag | Default in `crush run` (no markdown rendering) |
| Cache control | `--no-cache` flag | No conversation caching in `run` mode |
| Quiet mode | `-q` | `--quiet` (hides spinner) |
| Config format | YAML (`mods.yml`, manual) | JSON (`providers.json`, Catwalk registry) |
| Model catalog | Manual - add any model string | Curated - must exist in Catwalk or local override |
| API key config | `api-key-env` per provider in YAML | `$ENV_VAR` in providers.json (auto-discovered) |

## Model Availability Gap

Tested April 8, 2026 with crush v0.55.1 and latest Catwalk providers.

| Provider | gcm model | In crush catalog? | crush equivalent |
|----------|-----------|-------------------|------------------|
| Cerebras | `qwen-3-235b-a22b-instruct-2507` | Yes | `cerebras/qwen-3-235b-a22b-instruct-2507` |
| Groq | `openai/gpt-oss-120b` | **No** | Closest: `groq/moonshotai/kimi-k2-instruct-0905` |
| Google | `gemini-3.1-flash-lite-preview` | **No** | Closest: `gemini/gemini-3-flash-preview` |
| Claude | haiku (via `claude` CLI) | Yes | `anthropic/claude-haiku-4-5-20251001` |

### Crush Catwalk catalog (full)

**Groq** (2 models): `moonshotai/kimi-k2-instruct-0905`, `qwen/qwen3-32b`

**Gemini** (6 models): `gemini-3.1-pro-preview`, `gemini-3.1-pro-preview-customtools`, `gemini-3-pro-preview`, `gemini-3-flash-preview`, `gemini-2.5-pro`, `gemini-2.5-flash`

**Cerebras** (3 models): `gpt-oss-120b`, `qwen-3-235b-a22b-instruct-2507`, `zai-glm-4.7`

**Anthropic** (8 models): `claude-sonnet-4-6`, `claude-sonnet-4-5-20250929`, `claude-opus-4-6`, `claude-opus-4-5-20251101`, `claude-haiku-4-5-20251001`, `claude-opus-4-1-20250805`, `claude-opus-4-20250514`, `claude-sonnet-4-20250514`

### Can we add missing models?

The catalog lives at `~/.local/share/crush/providers.json`. It can be edited manually to add models. However, `crush update-providers` overwrites the file from the Catwalk repo, losing manual additions.

Workaround: contribute missing models to the [Catwalk repo](https://github.com/charmbracelet/catwalk) or maintain a local fork of providers.json and use `crush update-providers /path/to/local.json`.

## API Key Compatibility

Crush uses the same environment variables as mods for all providers we use:

| Provider | env var | mods | crush |
|----------|---------|------|-------|
| Groq | `GROQ_API_KEY` | via `api-key-env` in YAML | via `$GROQ_API_KEY` in providers.json |
| Cerebras | `CEREBRAS_API_KEY` | via `api-key-env` in YAML | via `$CEREBRAS_API_KEY` in providers.json |
| Google | `GEMINI_API_KEY` | via `api-key-env` in YAML (added in v2.6) | via `$GEMINI_API_KEY` in providers.json |
| Anthropic | `ANTHROPIC_API_KEY` | N/A (uses `claude` CLI) | via `$ANTHROPIC_API_KEY` in providers.json |

No changes to `~/.config/op/secrets.tpl` or `~/.secrets.env` needed.

## Verified Working (crush run)

```
echo "reply with just ok" | crush run --model cerebras/qwen-3-235b-a22b-instruct-2507 --quiet  # ok
echo "reply with just ok" | crush run --model gemini/gemini-3-flash-preview --quiet              # ok
echo "reply with just ok" | crush run --model groq/moonshotai/kimi-k2-instruct-0905 --quiet     # ok
echo "reply with just ok" | crush run --quiet                                                    # ok (default model)
```

## Options

### Option A: Stay on mods (do nothing)

- Mods binary works indefinitely (no server dependency, no license expiry)
- Models we need are all configured and tested as of v2.6
- Risk: no bug fixes, potential incompatibility with future OS or API changes
- Risk: `mods.yml` model catalog becomes stale over time (same problem we just fixed)

### Option B: Migrate to crush (full)

- Replace all `mods` calls with `crush run`
- Requires workaround for 2 missing models (Groq gpt-oss-120b, Gemini flash-lite-preview)
- Actively developed, receives provider/model updates via Catwalk
- Bonus: could consolidate Haiku provider onto crush too (drop `claude` CLI dependency)

### Option C: Migrate to crush (partial, accept model substitutions)

- Cerebras: `cerebras/qwen-3-235b-a22b-instruct-2507` - direct match, no change
- Groq: use `groq/moonshotai/kimi-k2-instruct-0905` or `groq/qwen/qwen3-32b` instead of gpt-oss-120b
- Google: use `gemini/gemini-3-flash-preview` instead of flash-lite-preview
- Haiku: optionally replace `claude` CLI with `anthropic/claude-haiku-4-5-20251001`
- Trade-off: different models may produce different commit message quality

### Option D: Drop mods, use raw API calls (curl)

- Maximum flexibility - any model on any provider
- More complex bash (`curl` + JSON construction + header management)
- No dependency on third-party CLI tools
- Hardest to maintain

## Recommendation

**Option A (stay on mods) for now, prepare for Option B.**

Rationale:
- mods works today, all four providers tested and functional as of v2.6
- The two missing crush models (`openai/gpt-oss-120b` on Groq, `gemini-3.1-flash-lite-preview`) are likely to appear in Catwalk soon - both are popular models on active providers
- No urgency - mods has no expiry mechanism, the binary will keep working
- Migration effort is small when ready (~10 lines of script changes)

### Migration trigger conditions (revisit when any of these happen)

1. Catwalk adds `openai/gpt-oss-120b` to Groq and `gemini-3.1-flash-lite-preview` to Gemini
2. A `mods` bug blocks a provider (no fix coming)
3. A provider changes their API in a way `mods` can't handle
4. macOS update breaks the `mods` binary

### Script changes when ready to migrate

```bash
# Before (mods)
llm_call() { echo "$1" | mods -a groq -m openai/gpt-oss-120b -r --no-cache -q 2>/dev/null; }

# After (crush)
llm_call() { echo "$1" | crush run --model groq/openai/gpt-oss-120b --quiet 2>/dev/null; }
```

The migration is a ~10-line change in the `case "$PROVIDER"` block - each `mods` call becomes a `crush run` call with the model format changed from `-a provider -m model` to `--model provider/model`.

## File References

| File | Purpose |
|------|---------|
| `git/git-commit-ai.sh` | Script with mods calls (lines 35, 39, 43) |
| `~/Library/Application Support/mods/mods.yml` | mods config (keep until migration) |
| `~/.local/share/crush/providers.json` | crush model catalog (Catwalk) |
| `git/2026-02-04-git-commit-ai-status.md` | gcm version history and status |
