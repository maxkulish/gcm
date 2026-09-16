# Grouping prompt overflows the model context window

- **Date:** 2026-09-16
- **Issues:** [CLO-797](https://linear.app/cloud-ai/issue/CLO-797/fix-grouping-prompt-exceeding-the-model-context-window-when-a-commit) (root cause), [CLO-798](https://linear.app/cloud-ai/issue/CLO-798/add-progress-and-failure-visibility-to-provider-calls-so-a-slow-or) (observability)
- **Version:** gcm 0.7.0+72e10a6
- **Assets:** [`clo-797-assets/`](clo-797-assets/) - capture server and raw measurements

## Symptom

Committing 515 files in `~/Work/investigations` (commit `f2e2ad0`):

```
gcm: Groq rejected the request (HTTP 400): Please reduce the length of the
messages or completion.. Likely an unsupported model/parameter or a gcm bug;
please report it.. Falling back to single-commit mode.
```

The grouping plan was lost; gcm produced a single commit for all 515 files.

The same input against Google does not 400 - Gemini's ~1M window accepts the
prompt Groq rejects - but it goes silent for 60 seconds and then times out
anyway. One input defect, two different bad experiences. See "The Gemini stall"
below for the measured breakdown.

## Root cause

The grouping prompt sends the same file path list **three times**, and none of
the three copies is bounded by `DiffBudget`.

`grouping_user_content` (`src/provider/facade.rs:365`) concatenates four
sections. Measured against the captured payload:

| Section | Tokens | Chars | chars/token | What it is |
|---|---:|---:|---:|---|
| `file_list` | 36,029 | 83,356 | 2.31 | JSON array of the 515 paths |
| `status` | 36,541 | 84,879 | 2.32 | JSON array of `"XY path"` - **the same paths again** |
| `stat` | 55 | 203 | 3.69 | |
| `body` - real diff content | 76,816 | ~236,000 | 2.91 | 50 untracked files actually read |
| `body` - name-only stubs | 39,649 | 102,607 | 2.59 | **the same paths a third time**, 463 `[content omitted: untracked cap reached]` blocks |
| system | 287 | 1,205 | 4.20 | `GROUPING_SYSTEM_PROMPT` |
| **total** | **189,377** | | | **1.44x the 131,072 window** |

**112,219 tokens - 59% of the prompt - are the same 515 paths repeated three
times.** The actual diff content is only 76,816 tokens. Deduplicating to one
copy yields ~113,200 tokens, which fits. Deduplication alone fixes this case.

### Why nothing caught it

1. **`gather_for_grouping` bounds only `body`.** `src/diff.rs:150-172` calls
   `cap_total(&mut body, budget.total_bytes)` and returns `file_list`, `status`
   and `stat` untouched. Those three grow linearly with file count and have no
   ceiling at any layer.

2. **The cap never fired.** Body was 338,834 bytes against a 350,000 budget, so
   `cap_total` was a no-op. This is not a budget-tuning problem - retuning the
   byte cap would not have prevented it.

3. **The third copy is emitted by the untracked guard itself.**
   `append_untracked` (`src/diff.rs:206`) is capped at `MAX_UNTRACKED_FILES = 50`,
   but past the cap every remaining file still gets a three-line named block.
   463 of 515 files took that branch. The cap bounds file *reads*, not prompt
   *size*.

### Latent second defect: the byte budget is miscalibrated

`DiffBudget::STANDARD_TOTAL = 350_000` (`src/diff.rs:54`) is wrong for Groq's
default model independent of the repetition bug. At the measured 2.91
chars/token for diff text, a full 350 KB body is **116,465 tokens - 89% of a
131,072 window**, before any other section and before the completion.

`docs/specs/2026-06-21-clo-489-provider-trait.md:160` records the value as
inherited "behavioral parity (O3)" from the original bash script, never derived
from a context window. The OpenAI budget on the very next line *was* derived
(`256_000` for a 128k window). Groq and OpenAI defaults are in the same window
class but carry budgets 94 KB apart.

Byte caps are a weak proxy for tokens in the first place. Measured spread in a
single payload: 2.31 chars/token for path JSON, 2.91 for diff body, 4.20 for
English prose - against the ~3.5-4 the byte caps implicitly assume. UUID-heavy
paths are the worst case and are exactly what large generated trees produce.

### Why the fallback worked, and why that is not reassuring

The single-commit prompt (`message_user_content`, `src/provider/facade.rs:374`)
omits `file_list` and `status` entirely, landing at ~116,520 tokens - roughly
11K under the limit. The fallback succeeded on a 9% margin, by luck.

A working tree whose diff body alone exceeds the window fails on **both** paths.
gcm then hard-errors instead of falling back, and the user gets the same
unactionable "Likely an unsupported model/parameter or a gcm bug" text.

## Reproduction

Deterministic and offline. `GCM_GROQ_BASE_URL` redirects gcm at a local capture
server, so no API key and no network are needed.

```bash
S=/tmp/gcm-repro
R=$S/repro

# 1. Rebuild the pre-commit worktree: parent tree committed, child tree overlaid
rm -rf "$R"; mkdir -p "$R"
git --git-dir=~/Work/investigations/.git archive f2e2ad0^ | tar -x -C "$R"
git --git-dir="$R/.git" --work-tree="$R" init -q "$R"
git --git-dir="$R/.git" --work-tree="$R" add -A
git --git-dir="$R/.git" --work-tree="$R" -c user.email=t@t -c user.name=t commit -q -m base
git --git-dir=~/Work/investigations/.git archive f2e2ad0 | tar -x -C "$R"
# yields: 2 modified, 513 untracked

# 2. Capture server on 127.0.0.1:8799 - writes the POST body, returns HTTP 400
CAP_OUT=$S/payload.json python3 docs/investigations/clo-797-assets/capture_server.py &

# 3. Run gcm against it
env -C "$R" GROQ_API_KEY=test GCM_GROQ_BASE_URL=http://127.0.0.1:8799 \
    GCM_RETRY_MAX=0 gcm --plan-only --json

# 4. Token-count the captured payload (tiktoken, o200k_base)
```

Raw output is in [`clo-797-assets/measurements.txt`](clo-797-assets/measurements.txt).

Because `--plan-only` still exercises the grouping call, the captured payload is
the grouping request (identifiable by the `response_format` json_schema and the
1,205-char `GROUPING_SYSTEM_PROMPT`).

## Fix directions

Ordered by ratio of effect to risk.

1. **Send each path once.** Merge `file_list` and `status` into one array of
   `{path, xy}` entries, and skip the name-only untracked stubs for paths
   already in that array. Removes ~76K tokens here at zero information loss.
2. **Bound the assembled prompt, not `body`.** Move the cap in
   `gather_for_grouping` to after assembly so all four sections share one
   budget. This is what makes the class of bug non-recurring at higher file
   counts, where even a deduplicated path list gets large.
3. **Derive the budget from the model's context window,** not from a
   per-provider byte constant. Budget in estimated tokens using a conservative
   2.3 chars/token, and reserve headroom for the completion.
4. **Degrade explicitly instead of 400.** When the assembled prompt still
   exceeds budget, drop hunks and group on paths + stat alone - grouping needs
   paths far more than it needs hunk bodies - and say so on stderr.

## The Gemini stall (CLO-798) - reproduced, cause established

Tracked separately as CLO-798. Measured against the same 515-file tree with
`gemini-3.1-flash-lite`:

| Stage | Wall time | How measured |
|---|---:|---|
| All local work (git status, diff gather, 50 file reads, prompt assembly) | 0.16s | local capture server, no egress |
| Grouping call, default 60s timeout | times out at 60.17s | real API, `GCM_LOG_LEVEL=debug` |
| Grouping call, `GCM_HTTP_TIMEOUT_SECS=300` | **succeeds in 85.5s** | real API |
| Fallback single-commit message call | ~1.9s | 62.07s total minus the 60.17s timeout |

**Nothing hangs.** The request needs ~85s; the default timeout is 60s. gcm
discards a minute of work and silently degrades to a worse result.

### Why grouping takes 85s when the message call takes 1.9s

`plan::schema()` (`src/plan.rs:310-331`) requires `groups[].files`, so the model
must **echo every changed path back** in its response. 515 paths is roughly 36K
output tokens. Generation dominates, not input processing - which is why 1.6x the
input costs 45x the wall time.

Grouping latency therefore scales with file count on the **output** side,
independently of the input bloat above. Groq's rejection text named both:
"reduce the length of the messages **or completion**". Fixing the input
duplication will not by itself make the 36K-token output fast.

### Anthropic cannot emit a large plan at all

`build_plan_payload` hardcodes `max_tokens: 4096` (`src/provider/anthropic.rs:161`).
A 515-file plan needs ~36K output tokens, so the response cannot fit. It hits
`stop_reason: max_tokens` and surfaces as "Anthropic response truncated; the diff
may be too large" (`anthropic.rs:242`) - a better message than Groq's, but still
a hard failure. Groq and Gemini set no output cap, so they just run long.

### The silence is structural

Every log call in the codebase is `debug_log!`, i.e. `Level::Debug`, which is
`Off` by default (`src/debug.rs:39`). Eight call sites in total:
`main.rs:415,462,495`, `plan.rs:279,294`, `http.rs:142,207,329`. With default
settings the generation path emits **zero** bytes for the full 60s, and there is
no progress indicator - the only spinner lives in the `gcm provider` wizard's
model-list fetch (`http.rs:33-37`).

With `GCM_LOG_LEVEL=debug` the output is genuinely useful and named the cause on
the first run: `Google API request timed out; falling back to single-commit`.
The information exists; it is switched off.

Relevant constants: timeout 60s (`DEFAULT_TIMEOUT_SECS`, `http.rs:20`);
`DEFAULT_MAX_RETRIES = 3`, so four attempts, backoff base 500ms / max 8s
(`http.rs:28-32`). `Timeout` is **not** retryable - `is_retryable`
(`identity.rs:97-99`) matches only `RateLimit` and `Server` - so one stall costs
exactly 60s, but a retried 429/5xx sequence can silently occupy ~4 minutes.

### Model-name note

The model reported as `gemini-3.5-flash-lite` is not the one that ran. The active
config has `gemini-3.1-flash-lite` enabled for google, and
`gcm --model gemini-3.5-flash-lite` fails fast and clearly ("not enabled for
google. Enabled: gemini-3.1-flash-lite") - that error path works well.
`ProviderId::Google.default_model()` is nonetheless `gemini-3.5-flash-lite`
(`identity.rs:456`), so the shipped default and the enabled list disagree.
