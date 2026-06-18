# Multi-Provider Git Commit AI (v2.3+)

## Summary

Extended `git-commit-ai.sh` to support multiple LLM providers via `mods` CLI, replacing the single Claude Haiku dependency for faster commit message generation.

## Aliases

| Alias | Provider | Model | Status |
|-------|----------|-------|--------|
| `gcm` | Claude Haiku (default) | `haiku` via `claude` CLI | Active |
| `gcmq` | Groq | `openai/gpt-oss-120b` | Active (migrated 2026-03-26 from Kimi K2) |
| `gcmg` | Google Gemini | `gemini-3.1-flash-lite-preview` | Active (added 2026-03-16) |
| `gcmc` | Cerebras | `qwen-3-235b-a22b-instruct-2507` | **Paused** - free tier rate limited (2026-03-16) |

All aliases support `--dry-run` and `--all` flags:
```bash
gcmg --dry-run    # test Gemini without committing
gcmq --all        # single commit mode via Groq
```

## How It Works

The script has a `--provider=` flag and an `llm_call()` function that dispatches to the right backend:

- **haiku**: `claude --model haiku -p "$prompt"` (existing behavior)
- **groq**: `echo "$prompt" | mods -a groq -m gpt-oss-120b -r --no-cache -q`
- **google**: `echo "$prompt" | mods -a google -m gemini-3.1-flash-lite-preview -r --no-cache -q`
- **cerebras**: `echo "$prompt" | mods -a cerebras -m qwen-3-235b-a22b-instruct-2507 -r --no-cache -q` (paused)

The `GCM_PROVIDER` env var can override the default provider globally.

## Provider-Specific Settings

### Context Limits (diff truncation)
- Haiku: 80K chars (~26K tokens)
- Cerebras: 400K chars
- Groq GPT OSS 120B: 350K chars (~90K tokens, 131K context window)
- Gemini 3.1 Flash Lite: 500K chars (model supports 1M tokens)

### mods Config (`~/Library/Application Support/mods/mods.yml`)

**Google section** - added `api-key-env` + new model (2026-03-16):
```yaml
google:
  api-key-env: GEMINI_API_KEY
  models:
    gemini-3.1-flash-lite-preview:
      aliases: ["gm3fl", "flash-3-lite", "gemini-3-flash-lite"]
      max-input-chars: 4000000
```

**Cerebras section** - current live models (as of 2026-03-16):
```yaml
cerebras:
  models:
    llama3.1-8b: ...
    qwen-3-235b-a22b-instruct-2507:
      aliases: ["qwen3-cerebras", "qwen3"]
      max-input-chars: 392000
```
Note: `gpt-oss-120b`, `zai-glm-4.7`, `llama3.1-70b` all return 404 - removed from config.

**Groq section** - GPT OSS 120B (migrated 2026-03-26):
```yaml
openai/gpt-oss-120b:
  aliases: ["gpt-oss-120b", "gpt-oss"]
  max-input-chars: 392000
moonshotai/kimi-k2-instruct-0905:  # DEPRECATED: removing April 15, 2026
  aliases: ["kimi-k2", "kimi"]
  max-input-chars: 780000
```

## API Keys

| Variable | Status |
|----------|--------|
| `CEREBRAS_API_KEY` | Set (in env) |
| `GROQ_API_KEY` | Set (in env) |
| `GEMINI_API_KEY` | Set (in env) |

## Dependencies

- `mods` v1.8.1 (`brew install charmbracelet/tap/mods`) - required for groq/google/cerebras providers
- `claude` CLI - required for haiku provider (unchanged)
- `jq`, `perl`, `file` - unchanged from v2.2

## Model Selection Rationale

- **Groq `gpt-oss-120b`**: Groq-recommended Kimi K2 replacement (500 t/s, $0.15/M input, 131K context). Previous model `kimi-k2` deprecated April 15, 2026
- **Gemini 3.1 Flash Lite**: lowest-latency Gemini model, 1M token context, free tier generous
- **Cerebras `qwen-3-235b-a22b-instruct-2507`**: 235B MoE model, fast inference - paused due to free tier rate limits

## Troubleshooting

```bash
# Debug any provider
DEBUG_GCM=1 gcmg --dry-run

# Test mods connection directly
echo "say ok" | mods -a google -m gemini-3.1-flash-lite-preview -r --no-cache -q
echo "say ok" | mods -a groq -m gpt-oss-120b -r --no-cache -q

# Check mods config
cat ~/Library/Application\ Support/mods/mods.yml | grep -A5 "google:"
```
