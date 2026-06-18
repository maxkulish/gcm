# Groq Model Selection + Thinking-Mode Handling (gcm v2.7)

> Status: shipped 2026-06-18. Adds selectable Groq models to `gcmq` and documents the
> reasoning-model ("thinking mode") pitfalls that a future native (Rust) rewrite must handle.
> Read alongside [2026-04-11-gcm-go-migration-plan.md](2026-04-11-gcm-go-migration-plan.md),
> which already specifies the full behavioural contract, and
> [2026-02-04-git-commit-ai-status.md](2026-02-04-git-commit-ai-status.md) for version history.

## Why this document exists

The `gcm` family wraps a single bash script (`git/git-commit-ai.sh`, symlinked to
`/opt/script/git-commit-ai.sh`) that asks an LLM to group changed files into logical commits and
write conventional-commit messages. Each alias selects a provider; until now each provider was
locked to one model. We wanted `gcmq` (Groq) to reach more than one model.

Adding `qwen/qwen3.6-27b` surfaced a class of problem the script was never designed for: reasoning
models that emit their chain-of-thought on stdout. The bash mitigation works, but the episode made
the limit of the bash design concrete, so the second half of this document captures everything a
native Rust port needs to do this correctly rather than by string-scraping.

---

# Part 1 - What shipped (gcm v2.7)

Four files changed. Two are in this repo, two live outside it.

## 1.1 Aliases (`~/.zshrc`)

`gcmq` keeps `gpt-oss-120b` as its default. Two numbered siblings set the model through an
environment variable that the script reads:

```bash
alias gcmq="/opt/script/git-commit-ai.sh --provider=groq"  # Groq GPT OSS 120B (default)
alias gcmq20="GCM_GROQ_MODEL=openai/gpt-oss-20b /opt/script/git-commit-ai.sh --provider=groq"  # Groq GPT OSS 20B
alias gcmq27="GCM_GROQ_MODEL=qwen/qwen3.6-27b /opt/script/git-commit-ai.sh --provider=groq"  # Groq Qwen3.6 27B
```

The `VAR=value command` prefix is exported into the script's environment for the duration of that
one invocation, which is standard POSIX behaviour. New aliases require `source ~/.zshrc` (or a new
terminal); the script and `mods.yml` edits are live immediately.

## 1.2 Script (`git/git-commit-ai.sh`) - the `groq` branch

The `groq` case now reads `GCM_GROQ_MODEL` (default `openai/gpt-oss-120b`), reflects the chosen
model in the status label, and strips qwen's thinking tags at the source:

```bash
groq)
    # GCM_GROQ_MODEL selects the Groq model (gcmq=default, gcmq20, gcmq27 set it)
    GROQ_MODEL="${GCM_GROQ_MODEL:-openai/gpt-oss-120b}"
    PROVIDER_LABEL="Groq ${GROQ_MODEL}"
    # qwen3.6 is a thinking model and prints <think>…</think> to stdout; strip it so it
    # pollutes neither the JSON plan nor the commit message (no-op for the gpt-oss models)
    llm_call() { echo "$1" | mods -a groq -m "$GROQ_MODEL" -r --no-cache -q 2>/dev/null | perl -0777 -pe 's{<think>.*?</think>\s*}{}gs'; }
    ;;
```

Putting the strip inside `llm_call` cleans **both** downstream consumers in one place: the grouping
JSON plan and the single-commit fallback message. The other provider branches (haiku, google,
cerebras) are untouched.

The usage comment at the top of the script gained a line:

```bash
#   Groq model override: GCM_GROQ_MODEL=<id> (default openai/gpt-oss-120b; e.g. openai/gpt-oss-20b, qwen/qwen3.6-27b)
```

## 1.3 `mods` model registration (`~/Library/Application Support/mods/mods.yml`)

`mods` validates `-m <id>` against its config and errors on an unknown id, so both new models had
to be registered under `groq.models` (next to the existing `openai/gpt-oss-120b`):

```yaml
openai/gpt-oss-20b:
  aliases: ["gpt-oss-20b"]
  max-input-chars: 392000 # 131K
qwen/qwen3.6-27b:
  aliases: ["qwen3.6-27b", "qwen3.6"]
  max-input-chars: 392000 # 131K
  max-completion-tokens: 32768 # 32,768
```

Alias choices avoid collisions: `gpt-oss` is already claimed by the 120b entry and `qwen3` by the
Cerebras `qwen-3-235b` entry, so we used distinct names.

## 1.4 Context tier - why `MAX_DIFF` was left alone

The script truncates the diff per provider (`groq) MAX_DIFF=350000`). On Groq all three models
expose the same 131,072-token context window, so 350,000 chars (~131K tokens at the script's rough
3-chars-per-token assumption) stays correct for every Groq model. No `MAX_DIFF` change was needed.

Note that qwen3.6-27b is *natively* a 262K-context model; Groq serves it at 131K. Do not assume the
native window when sizing prompts against Groq.

## 1.5 Usage quick reference

```
gcmq      → openai/gpt-oss-120b   (default, unchanged)
gcmq20    → openai/gpt-oss-20b
gcmq27    → qwen/qwen3.6-27b
gcm       → claude haiku          (unchanged)
gcmg      → gemini-3.1-flash-lite (unchanged)
```

Ad-hoc model without an alias:

```bash
GCM_GROQ_MODEL=<groq-model-id> gcmq          # any registered groq model
gcmq27 --dry-run                             # preview the plan, commit nothing
gcmq27 --reset                               # discard the cached plan and re-analyse
```

---

# Part 2 - Thinking-mode intricacies

This is the part that matters for the rewrite. "Thinking mode" (reasoning) is not uniform across
Groq models, and the differences are invisible until a model is actually wired in.

## 2.1 Two reasoning-output conventions on Groq

Groq serves reasoning models, but how the reasoning reaches the client differs by model family.
Verified against Groq's reasoning docs and live calls:

| Model | Where reasoning goes by default | Stdout through `mods -r` |
|-------|--------------------------------|--------------------------|
| `openai/gpt-oss-120b` / `-20b` | a separate `reasoning` field on the message | clean - only the final answer |
| `qwen/qwen3.6-27b` | inline in the content as `<think>…</think>` (Groq's default `reasoning_format` is `raw`) | polluted - the whole think block precedes the answer |

This is the crux. The two gpt-oss models that `gcmq`/`gcmq20` use route reasoning through a side
channel, so the content `mods` prints is already clean. qwen3.6 instead inlines its reasoning into
the content stream, so it lands on stdout.

## 2.2 Why the think block defeats `2>/dev/null`

The script's `llm_call` ends in `2>/dev/null`, which suppresses **stderr** only. qwen's `<think>`
block is on **stdout**, the same stream as the answer, so the redirect does nothing to it. Live
proof from this session (stderr discarded, exactly as the script does it):

```
$ echo "Reply with exactly: OK" | mods -a groq -m qwen/qwen3.6-27b -r --no-cache -q 2>/dev/null
<think>
Here's a thinking process:
1. Analyze User Input ...
</think>

OK
```

## 2.3 Failure mode A - the grouping plan stops parsing

The script extracts the JSON plan with a greedy match:

```bash
JSON_BODY=$(echo "$LLM_RESPONSE" | sed '/^```/d' | perl -0777 -ne 'print $1 if /(\{.*\})/s' | jq '.' 2>/dev/null)
```

`(\{.*\})` with `/s` matches from the **first** `{` to the **last** `}`. A reasoning model writing
about JSON structure puts literal `{` braces inside its think block, so the match begins inside the
reasoning text and captures `{...think prose...} ... {real json}`. That is not valid JSON, `jq`
fails, and the script falls back to a single lumped commit, silently losing the grouping feature
that is the whole point of the tool.

## 2.4 Failure mode B - the commit message gets the reasoning pasted in

The single-commit fallback (`fallback_single_commit`) uses the model output verbatim:

```bash
commit_msg=$(llm_call "...")
git commit -S -m "$commit_msg"
```

Without stripping, the commit message itself would contain `<think>…</think>` followed by the real
message. This is the worse failure because it writes garbage into history rather than degrading
gracefully.

## 2.5 The bash mitigation and its limits

The fix strips the think block in `llm_call` before anything downstream sees it:

```bash
perl -0777 -pe 's{<think>.*?</think>\s*}{}gs'
```

- `-0777` slurps the whole input as one string (so the regex spans newlines).
- `s{…}{}gs`: `g` removes every think block, `s` (dotall) lets `.` cross newlines.
- `.*?` is non-greedy, so it stops at the first `</think>` rather than eating past a later one.
- `\s*` also consumes the blank lines after the close tag, so the JSON/answer starts clean.
- It is a no-op for gpt-oss, whose stdout has no `<think>` tags.

**Limits a regex strip cannot escape, and which the bash version still carries:**

1. **Unterminated think block.** If `max-completion-tokens` cuts the response off mid-reasoning,
   there is no `</think>`. The non-greedy pattern then matches nothing and the whole block survives,
   so extraction fails and the run falls back. Capping completion tokens reduces the odds but cannot
   remove this.
2. **Wasted tokens and latency.** The model still *generates* the full reasoning; the strip only
   discards it after the fact. For a one-line commit message this is pure overhead - real money and
   seconds spent producing text that is thrown away.
3. **No access to the structured answer.** `mods` merges everything into stdout. The script cannot
   ask for "just the parsed answer" because it never sees the structured response, only text.

## 2.6 The proper fix - native API parameters (what bash cannot reach)

Groq's OpenAI-compatible API exposes parameters that make every problem in 2.3-2.5 disappear, but
`mods` does not pass them through. Verified from Groq's reasoning and API-reference docs:

| Parameter | Applies to | Effect |
|-----------|-----------|--------|
| `reasoning_effort: "none"` | **qwen3.6-27b** | disables reasoning entirely - no thinking tokens generated. Fastest, cheapest, nothing to strip. |
| `reasoning_effort: "low"\|"medium"\|"high"` | gpt-oss-20b/120b | tunes effort (`medium` default); gpt-oss cannot be fully disabled (CoT-trained). |
| `reasoning_format: "hidden"` | qwen-family | the model still reasons, but the reasoning is not returned. |
| `reasoning_format: "parsed"` | qwen-family | reasoning is split into a dedicated `message.reasoning` field; content stays clean. |
| `reasoning_format: "raw"` | qwen-family | reasoning inlined in `<think>` tags. **This is the Groq default**, which is why qwen polluted stdout. |
| `include_reasoning: false` | gpt-oss | excludes the `reasoning` field from the response (mutually exclusive with `reasoning_format`). |
| `response_format: {type:"json_object"}` | all | JSON mode - the model must emit valid JSON. Requires the word "JSON" in the prompt (the current prompt already qualifies). |
| `response_format: {type:"json_schema", json_schema:{…}}` | newer models only | Structured Outputs - guaranteed schema match. Preferred where supported. |

Two interactions to remember:

- With JSON mode or tool calls, `reasoning_format` **must** be `parsed` or `hidden`; `raw` returns
  HTTP 400. So the grouping call should combine `response_format: json_object` with either
  `reasoning_effort: none` (qwen) or `reasoning_format: hidden`.
- gpt-oss does not support `reasoning_format`; use `include_reasoning: false` instead.

---

# Part 3 - Why the bash script is insufficient

The thinking-mode episode is one instance of a structural ceiling. The script talks to models only
through `mods`, a thin CLI that exposes account + model + a few flags and nothing else. That forces
every hard problem into text manipulation:

- **Parsing.** The plan is recovered with `sed` + `perl` + `jq` heuristics (strip fences, greedy
  `{…}`, validate). Any reasoning text, prose preamble, or stray brace breaks it. JSON mode would
  make this exact and trivial, but `mods` cannot request it.
- **Reasoning control.** The cleanest fixes (`reasoning_effort: none`, `reasoning_format: hidden`,
  `include_reasoning: false`) are unreachable. The script can only scrub output after generation.
- **Error handling.** A failed call collapses to "empty response → single-commit fallback." There is
  no way to distinguish a 429 rate limit from a 400 bad-parameter from a truncated stream, so
  retries and useful diagnostics are impossible.
- **Diff sanitation.** `_safe_diff()` is a 30-line embedded perl program detecting binary bodies by
  non-printable-byte ratio - the kind of logic that is a few typed lines in Rust with proper
  streaming and tests.
- **Multi-model fan-out.** Adding a model means a new alias plus a `mods.yml` entry plus a script
  branch plus a `MAX_DIFF` arm. The coupling is implicit and easy to get wrong (an unset `MAX_DIFF`
  under `set -u` is a runtime error).

None of these are bugs in the script; they are the cost of orchestrating reasoning-capable APIs from
shell.

---

# Part 4 - Guidance for the Rust port

The behavioural contract (flags, cache semantics, grouped vs single-commit flow, signed commits,
dry-run, fallbacks) is already specified task-by-task in
[2026-04-11-gcm-go-migration-plan.md](2026-04-11-gcm-go-migration-plan.md). The Rust port inherits
that contract. This section records only what the thinking-mode work adds on top.

## 4.1 Talk to provider APIs directly, drop `mods`

Use a native async HTTP client (`reqwest` + `tokio`) against each provider's endpoint:

- Groq: `POST https://api.groq.com/openai/v1/chat/completions` (OpenAI-compatible), `GROQ_API_KEY`.
- Anthropic (haiku): Messages API, or keep shelling to the `claude` CLI if simpler at first.
- Google (gemini): `generativelanguage.googleapis.com`, `GEMINI_API_KEY`.

This removes the `mods` and `mods.yml` dependency entirely and unlocks every parameter in 2.6.

## 4.2 Make parsing exact, not heuristic

Define the plan as a serde type and request JSON mode so the model must return it:

```rust
#[derive(Deserialize)]
struct Plan { groups: Vec<Group> }
#[derive(Deserialize)]
struct Group {
    files: Vec<String>,
    summary: String,
    commit_message: Option<String>, // populated only for groups[0], as today
}
```

Send `response_format: {"type":"json_object"}` (the prompt already says "Output ONLY valid JSON", so
JSON mode's keyword requirement is satisfied). Prefer `json_schema` Structured Outputs on models
that support it. This deletes the entire `sed`/`perl`/`jq` extraction stage and both its failure
modes from 2.3.

## 4.3 Suppress reasoning at the source, per model

Pick the parameter by model family rather than scrubbing output:

| Model family | Set on the request |
|--------------|--------------------|
| `qwen/qwen3.6-27b` | `reasoning_effort: "none"` (no thinking) - or `reasoning_format: "hidden"` if some reasoning quality is wanted. With JSON mode, never `raw`. |
| `openai/gpt-oss-*` | `include_reasoning: false` (content is already clean; this just drops the side field). `reasoning_effort: "low"` to trim cost. |

Keep a regex `<think>` strip as a defensive fallback only, since a misconfigured request or a future
model could still inline reasoning. Belt and suspenders, not the primary mechanism.

## 4.4 Structured errors and retries

Map HTTP status to typed errors: 429 → backoff + retry, 400 → surface the bad parameter (do not
silently fall back), 5xx → retry, empty/short body → the existing single-commit fallback. This is
where the native client earns its keep over the current "any failure → fallback" behaviour.

## 4.5 Carry over, do not redesign

Preserve exactly: per-provider context caps (`MAX_DIFF`), the binary-diff elision logic
(`_safe_diff`), the per-repo plan cache keyed by `sha256(repo-root)` at
`/tmp/gcm-plan-<key>.json` and its advance-on-commit semantics, untracked-file inclusion, renamed
file handling (new path), the `-S` signed commit, and the interactive `Y/n/e` prompt with `$EDITOR`.
The model-selection surface (`GCM_GROQ_MODEL` plus the numbered aliases) maps naturally to a
`--model` flag and/or a small config file.

## 4.6 Suggested crate stack

`reqwest` (rustls) + `tokio` for HTTP, `serde`/`serde_json` for the plan, `clap` for flags,
`git2` (libgit2) or shelling to `git` for staging/commit, `anyhow`/`thiserror` for errors. Keep the
provider behind a trait so haiku/groq/google share one flow, mirroring today's `llm_call` indirection.

---

# Verification performed (2026-06-18)

All checks were run live, not assumed:

- `bash -n git-commit-ai.sh` - clean before and after the edits.
- `python3 -c "yaml.safe_load(...)"` on `mods.yml` - valid.
- `openai/gpt-oss-20b` and `qwen/qwen3.6-27b` each resolved, authenticated, and returned output
  through `mods -a groq`.
- Stream isolation: confirmed qwen's `<think>` block arrives on **stdout** (survives `2>/dev/null`),
  while gpt-oss stdout is clean.
- Strip pipeline: qwen output reduced to a bare `OK`; gpt-oss unchanged (no-op).
- Full `gcmq27 --dry-run --reset` on a real working-tree diff produced a well-formed single-group
  plan and a clean conventional-commit message with zero `<think>` leakage. Dry-run committed
  nothing; the test plan cache was removed afterward.

# References

- Groq reasoning docs: https://console.groq.com/docs/reasoning (reasoning_format / reasoning_effort / include_reasoning)
- Groq API reference: https://console.groq.com/docs/api-reference (response_format json_object / json_schema)
- Model: https://console.groq.com/docs/model/qwen/qwen3.6-27b (131K context on Groq, 32K max output)
- Prior contract + port plan: [2026-04-11-gcm-go-migration-plan.md](2026-04-11-gcm-go-migration-plan.md)
- Provider/alias reference: [2026-02-27-multi-provider-gcm.md](2026-02-27-multi-provider-gcm.md)
- Living status + version history: [2026-02-04-git-commit-ai-status.md](2026-02-04-git-commit-ai-status.md)
