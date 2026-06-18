# Git Commit AI — Status

## Current State: ✅ v2.7 Deployed (Selectable Groq Models + Thinking-Mode Handling)

## What Exists

| Component | Path | Notes |
|-----------|------|-------|
| Script (source) | `git/git-commit-ai.sh` | 350 lines, bash |
| Script (deployed) | `/opt/script/git-commit-ai.sh` | Symlink → source |
| Shell alias | `gcm` | Points to `/opt/script/git-commit-ai.sh` |
| Docs | `zsh/2026-01-25-git-commit-ai.md` | Full workflow + architecture docs |

## Version History

### v1 (2026-01-25)
- Stage everything → Claude generates message → confirm → commit
- Single Haiku call, no grouping

### v2 (2026-02-04)
- **Grouped commits**: Haiku analyzes all changes, groups by semantic relevance
- Commits one group at a time; user runs `gcm` again for remaining groups
- `--all` flag to bypass grouping (original v1 behavior)
- `--dry-run` works with both modes
- Robust fallbacks: invalid JSON, hallucinated filenames, empty responses → single commit mode
- File validation against `git status` output
- Proper rename/delete handling via `git ls-files --error-unmatch`

### v2.1 (2026-02-08)
- **Debug mode**: `DEBUG_GCM=1 gcm` logs raw Haiku response and extracted JSON to stderr
- Helps diagnose JSON parsing failures without modifying script logic
- Debug output shows three-stage extraction pipeline: raw response → JSON extraction → validation

### v2.2 (2026-02-10)
- **Binary file safety**: skip binary files when building untracked file pseudo-diffs
- Detect text vs binary via `file --brief | grep -qi text`
- Binary files included as `+[binary file]` placeholder (Haiku still sees them for grouping)
- `LC_ALL=C` on `sed` as belt-and-suspenders for stray non-UTF-8 bytes in text files
- **Root cause**: PDFs in `aws-ri/alerts/` triggered `sed: RE error: illegal byte sequence` on macOS
- macOS BSD `sed` is strict about locale encoding — binary bytes are invalid UTF-8

### v2.3 (2026-02-27)
- **Multi-provider support**: `gcmq` (Groq), `gcmg` (Gemini), `gcmc` (Cerebras)
- Provider dispatch via `mods` CLI; `--provider=` flag and `GCM_PROVIDER` env var
- Per-provider diff truncation limits
- See `2026-02-27-multi-provider-gcm.md` for full details

### v2.4 (2026-03-02)
- **LLM plan caching**: save grouping plan after first LLM call, skip LLM on re-runs
- Cache: `/tmp/gcm-plan-<sha256>.json`, shared across providers, auto-deleted on last commit
- `--reset` flag to force re-analysis
- See `2026-03-02-gcm-cache-status.md` for full details

### v2.5 (2026-03-26)
- **Groq model migration**: `kimi-k2` -> `openai/gpt-oss-120b` (Kimi K2 deprecated April 15, 2026)
- GPT OSS 120B: 500 t/s, $0.15/M input, 131K context window
- Diff truncation limit reduced from 500K to 350K (matching smaller context)
- Kimi K2 entry kept in mods.yml with deprecation comment until April 15

### v2.6 (2026-04-08)
- **Provider audit and fix**: all three alternative providers (Groq, Cerebras, Google) were broken
- **Groq**: model name fixed from `gpt-oss-120b` to `openai/gpt-oss-120b` (namespaced on Groq API)
- **Cerebras**: model `qwen-3-235b-a22b-instruct-2507` was correct in script but missing from mods.yml config
- **Google/Gemini**: `gemini-3.1-flash-lite-preview` was correct in script but missing from mods.yml; added `api-key-env: GEMINI_API_KEY` (was absent, causing silent auth failure)
- **New alias**: `gcmg` added to .zshrc for `--provider=google`
- **Root cause**: `mods` validates model names against its YAML config before calling the API; script's `2>/dev/null` swallowed the client-side rejection, making it look like an empty LLM response

### v2.7 (2026-06-18)
- **Selectable Groq models**: `groq` branch now reads `GCM_GROQ_MODEL` (default `openai/gpt-oss-120b`). New aliases `gcmq20` (`openai/gpt-oss-20b`) and `gcmq27` (`qwen/qwen3.6-27b`) set it; `gcmq` unchanged.
- **mods.yml**: registered `openai/gpt-oss-20b` and `qwen/qwen3.6-27b` under `groq.models` (mods rejects unregistered ids). All three share Groq's 131K context tier, so `MAX_DIFF` stayed 350K.
- **Thinking-mode handling**: `qwen/qwen3.6-27b` is a reasoning model that inlines `<think>…</think>` on **stdout** (gpt-oss models route reasoning through a separate field, so their stdout is clean). The `2>/dev/null` does not catch stdout, so unstripped it broke JSON-plan extraction (think text contains `{` braces) and leaked into commit messages. Fix: the `groq` `llm_call` pipes through `perl -0777 -pe 's{<think>.*?</think>\s*}{}gs'` (no-op for gpt-oss).
- **Rust-port foundation**: full write-up of the changes, the thinking-mode failure modes, and the native Groq API parameters (`reasoning_effort`, `reasoning_format`, `response_format`) that obviate the regex/parse hacks in [2026-06-18-groq-model-selection-and-thinking-mode.md](2026-06-18-groq-model-selection-and-thinking-mode.md).

## Architecture

```
gcm → gather changes (no staging) → single Haiku call → parse JSON → display groups
    → user confirms → stage group 1 only → git commit -S → report remaining
```

### Single Haiku Call
- Input: file list + `git status --porcelain` + diff stats + full diff (80K cap)
- Output: JSON `{"groups": [{"files":[], "summary":"", "commit_message":""}]}`
- commit_message only on groups[0]; null on others

### Dependencies
- `claude` CLI with `--model haiku` (default provider)
- `mods` CLI for alternative providers (Groq, Cerebras, Google)
- `jq` for JSON parsing
- `perl` for robust JSON extraction from LLM output
- Git with GPG signing (`-S`)

### Provider Matrix (v2.7)

| Provider | Alias | Model | Context | Diff Limit |
|----------|-------|-------|---------|------------|
| Claude Haiku | `gcm` | `haiku` (via `claude` CLI) | 200K | 80K |
| Cerebras | `gcmc` (alias commented out since 2026-03-16) | `qwen-3-235b-a22b-instruct-2507` | 65K | 400K |
| Groq | `gcmq` | `openai/gpt-oss-120b` (default) | 131K | 350K |
| Groq | `gcmq20` | `openai/gpt-oss-20b` | 131K | 350K |
| Groq | `gcmq27` | `qwen/qwen3.6-27b` (thinking; `<think>` stripped) | 131K | 350K |
| Google | `gcmg` | `gemini-3.1-flash-lite` | 1M | 500K |

Groq model is overridable for any of the `gcmq*` aliases via `GCM_GROQ_MODEL=<id> gcmq`.

## Key Technical Decisions

| Decision | Rationale |
|----------|-----------|
| File-level grouping, not hunk-level | Simpler; covers 90% of cases |
| Single Haiku call (group + message) | Minimize latency; one round-trip |
| Fallback on any parse error | Never block the commit workflow |
| `perl -0777` for JSON extraction | Handles fences, preamble, trailing text |
| `jq <<<` instead of `echo \| jq` | Correct `pipefail` behavior |
| No RENAME_MAP | `git add new_path` detects renames automatically |
| `git ls-files --error-unmatch` for deletes | Only `git rm` files that are actually tracked |
| 80K diff truncation | Stay within Haiku context window; file list + stats always complete |
| Debug output to stderr (`>&2`) | Doesn't pollute stdout/script logic; works with pipes |
| Environment variable debug trigger | Non-invasive; no script modification needed for debugging |
| `file --brief` for binary detection | Catches PDFs, images, data files before piping to `sed` |
| `LC_ALL=C` on `sed` | macOS BSD sed errors on non-UTF-8 bytes; C locale treats as raw bytes |
| `+[binary file]` placeholder | Haiku still sees file exists for grouping; no wasted tokens on binary data |
| Models must be in mods.yml | `mods` validates against its YAML config before calling the API; missing entries fail silently with `2>/dev/null` |
| `api-key-env` per provider | Without it, `mods` cannot find the API key even if the env var exists |

## Bash Tricks and Patterns

### Conditional Debug Logging
```bash
if [[ -n "${DEBUG_GCM:-}" ]]; then
    echo "DEBUG: message" >&2
fi
```
- `${VAR:-}` expands to empty string if unset (prevents `set -u` errors)
- `-n` tests for non-empty string (true if `DEBUG_GCM=1` or any non-empty value)
- `>&2` redirects to stderr (preserves stdout for script output/pipes)

### Three-Stage LLM Response Extraction
```bash
# Stage 1: Strip markdown fences
# Stage 2: Extract outermost JSON block
# Stage 3: Validate with jq
JSON=$(echo "$RESPONSE" | sed '/^```/d' | perl -0777 -ne 'print $1 if /(\{.*\})/s' | jq '.' 2>/dev/null) || true

# Fallback to raw if extraction failed
if [[ -z "$JSON" ]] || ! jq -e '.expected_key' <<< "$JSON" &>/dev/null; then
    JSON="$RESPONSE"
fi
```
- `perl -0777` slurp mode (reads entire input as single string, enabling multiline regex)
- `.*` with `/s` modifier makes `.` match newlines (greedy match to outermost braces)
- `|| true` prevents script exit on failure (when using `set -e`)
- `jq -e` exits 1 if expression is false/null/empty (testable with `if`)

### Validation Against Known Set
```bash
declare -A VALID_FILES=()
for f in "${ALL_FILES[@]}"; do
    VALID_FILES["$f"]=1
done

# Check if key exists in associative array
if [[ -z "${VALID_FILES[$hf]+x}" ]]; then
    echo "Unknown file: $hf"
fi
```
- `${VALID_FILES[$key]+x}` expands to "x" if key exists, empty otherwise
- `-z` tests for empty string (true if key doesn't exist)
- More efficient than looping for membership testing (O(1) vs O(n))

## Known Limitations

- File-level only — can't split hunks within a single file
- Haiku may produce suboptimal groupings for very large changesets (20+ files)
- No interactive group reordering (always commits group 1 first)
- Untracked file content capped at 8K per file in the pseudo-diff

## Troubleshooting

### JSON Parse Failures

**Error**: `⚠️  Failed to parse Haiku response as valid JSON. Falling back to single commit.`

This occurs when the three-stage extraction pipeline fails:
1. **Stage 1**: `sed '/^```/d'` strips markdown fences
2. **Stage 2**: `perl -0777 -ne 'print $1 if /(\{.*\})/s'` extracts outermost `{...}` block
3. **Stage 3**: `jq -e '.groups | length > 0'` validates JSON structure

#### Common Causes

| Pattern | Example | Why It Fails |
|---------|---------|--------------|
| **Explanatory text only** | `I cannot group these files because...` | No JSON in response → perl returns empty |
| **Malformed JSON strings** | `"commit_message": "fix: escape "quotes" issue"` | Unescaped quotes → jq parse error |
| **Empty groups array** | `{"groups": []}` | Valid JSON but length check fails |
| **Nested response object** | `{"response": {"groups": [...]}}` | `.groups` path doesn't exist |
| **API timeout/error** | Silent failure, truncated response | Invalid JSON syntax |

#### Debug Steps

1. **Enable debug mode** to see raw Haiku output:
   ```bash
   DEBUG_GCM=1 gcm --dry-run
   ```

2. **Look for debug markers** in stderr:
   ```
   ════════════════════════════════════════════════════════════
   DEBUG: Raw Haiku response:
   [actual response here]
   ════════════════════════════════════════════════════════════
   DEBUG: Extracted JSON_BODY:
   [extracted JSON or empty if extraction failed]
   ════════════════════════════════════════════════════════════
   ```

3. **Compare raw vs extracted**:
   - If raw response has no `{...}` block → Haiku refused/failed to generate JSON
   - If extracted is empty but raw has JSON → perl regex failed (unexpected format)
   - If extracted has JSON but script still fails → `.groups` validation failed

#### Workarounds

- **Bypass grouping**: `gcm --all` (uses v1 single-commit fallback)
- **Manual commit**: Stage files yourself, then use `git commit` directly
- **Retry**: Sometimes Haiku produces better output on second attempt

### Binary File / Illegal Byte Sequence

**Error**: `sed: RE error: illegal byte sequence`

Triggered when untracked files contain non-UTF-8 bytes (PDFs, images, compiled files). macOS BSD `sed` is strict about locale encoding.

**Fixed in v2.2**: Binary files are detected via `file --brief | grep -qi text` and replaced with a `+[binary file]` placeholder. Text files use `LC_ALL=C sed` as safety net.

### File Validation Failures

**Error**: `⚠️  Haiku hallucinated filenames. Falling back to single commit.`

Haiku suggested a file that doesn't exist in `git status --porcelain` output. This is a safeguard against hallucinations. No user action needed — script auto-falls back.

## Testing Checklist

- [ ] `gcm --dry-run` with changes across 2+ unrelated folders
- [ ] `gcm` — confirm group 1 commits, remaining reported
- [ ] `gcm` again — picks up group 2
- [ ] `gcm --all` — single commit, all files
- [ ] Renamed file handling
- [ ] Deleted file handling
- [ ] JSON parse failure fallback (manually corrupt response)
- [ ] Debug mode: `DEBUG_GCM=1 gcm --dry-run` shows raw response

## Usage Quick Reference

| Command | Description |
|---------|-------------|
| `gcm` | Group changes, commit first group (Claude Haiku) |
| `gcmc` | Same as gcm, via Cerebras (Qwen3 235B) |
| `gcmq` | Same as gcm, via Groq (GPT OSS 120B) |
| `gcmg` | Same as gcm, via Google (Gemini 3.1 Flash Lite) |
| `gcm --dry-run` | Preview grouping without committing |
| `gcm --all` | Bypass grouping, single commit (v1 mode) |
| `gcm --reset` | Force re-analysis, discard cached plan |
| `DEBUG_GCM=1 gcm --dry-run` | Show debug output (raw LLM response + extraction) |
| `gcm` (run again) | Commit next group from previous run |

## Key Files Modified

- `git/git-commit-ai.sh:207-222` — Debug logging blocks (v2.1)
- `git/git-commit-ai.sh:145-158` — Binary file detection + LC_ALL=C sed (v2.2)
- `git/2026-02-04-git-commit-ai-status.md` — This status file
