# Design: CLO-797 — Bound the grouping prompt to the model's context window

**Linear task**: https://linear.app/cloud-ai/issue/CLO-797
**Status**: Finalized
**Created**: 2026-09-17
**Finalized**: 2026-09-17
**Approved by**: Max
**Investigation**: [`docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md`](../investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md)

## Problem

A 515-file working tree produced a 189,377-token grouping prompt against
`openai/gpt-oss-120b`, whose window is 131,072. Groq returned HTTP 400 and gcm fell back
to single-commit mode, losing the grouping feature exactly where it is most valuable.

The prompt sends the same 515 paths three times, and none of the three copies is bounded:

| Section | Tokens | What it is |
| -- | --: | -- |
| `file_list` | 36,029 | JSON array of the paths |
| `status` | 36,541 | JSON array of `"XY path"` — the same paths again |
| `stat` | 55 | |
| `body` — real diff content | 76,816 | the 50 untracked files actually read |
| `body` — name-only blocks | 39,649 | the same paths a third time, as 463 `[content omitted: untracked cap reached]` stubs |
| system | 287 | |
| **total** | **189,377** | 1.44× the window |

112,219 tokens (59%) are the repeated path list. Actual diff content is 76,816.

Two structural reasons this was not caught:

1. `gather_for_grouping` (`src/diff.rs:203-226`) calls `cap_total` on `body` only.
   `file_list`, `status` and `stat` are returned uncapped and grow linearly with file count.
2. The cap never fired anyway — body was 338,834 bytes against a 350,000-byte budget.
   This is not a budget-tuning problem.

The third copy comes from `append_untracked` (`src/diff.rs:256-261`): past
`MAX_UNTRACKED_FILES = 50`, every remaining file still gets a 3-line named block. 463 of
515 files took that path.

**Latent second defect.** `DiffBudget::STANDARD_TOTAL = 350_000` bytes (`src/diff.rs:56`)
is miscalibrated. At the measured 2.91 chars/token for diff text, a full 350 KB body is
116,465 tokens — 89% of a 131,072 window before anything else or the completion.
`docs/specs/2026-06-21-clo-489-provider-trait.md:160` records it as inherited "behavioral
parity (O3)", never derived from a window; the OpenAI budget on the same line *was*
derived. Byte caps are a weak proxy for tokens: 2.31 chars/token measured for path JSON
against 2.91 for diff body, both far below the ~3.5-4 the byte caps implicitly assume.

**Why the fallback saved this run, and why that is luck.** The single-commit prompt omits
`file_list` and `status`, landing at ~116,520 tokens — about 11K under the limit. A repo
whose diff body alone exceeds the window fails on *both* paths and hard-errors.

## Goals / Non-goals

**Goals**

- AC1: the 515-file reproduction produces a grouping prompt under the model's window.
- AC2: **the prompt carries exactly one path inventory.** No second path list and no
  per-file stub duplicates it. Paths still appear inside `body` as part of real
  `diff --git` headers — that is diff content, not a redundant copy. (Revised after review;
  see "AC2 as originally written" below.)
- AC3: a regression test asserts the *rendered* prompt — `grouping_user_content` output
  plus the system prompt and envelope allowance, not a sum of struct fields — respects
  the budget.
- AC4: when the prompt cannot be made to fit, gcm emits an actionable error and never a
  provider HTTP 400. "Cannot be made to fit" is defined exactly by the D3 ladder: for
  grouping, when the path inventory plus `stat` alone exceed the budget; for the
  single-commit path, when the diff alone exceeds it.
- Keep the CLO-487 path-agreement contract: the model must still see every changed path
  exactly as gcm will validate and stage it, newline-safe, renames included.
- A pre-flight failure makes **zero** HTTP requests.

**Non-goals**

- Tokenizing for real. gcm ships no tokenizer and will not add one; the budget stays a
  character estimate with a conservative divisor and a stated safety margin.
- Changing how the single-commit / message prompt is **gathered or truncated**
  (CLO-486 O3 parity). Adding the shared pre-flight check to that path *is* in scope —
  it is the only way AC4 can hold, since the single-commit path is where a
  window-exceeding diff currently hard-errors. (Reworded after review: the earlier
  blanket non-goal contradicted D5.)
- Reworking the plan *output* schema so groups reference indices instead of echoing paths
  — that is CLO-801, the output side of the same call. This design only has to *reserve*
  space for today's path-echoing output.
- Changing `MAX_UNTRACKED_FILES` / `MAX_UNTRACKED_BYTES` (FR-57 caps).

### AC2 as originally written

The Linear ticket states AC2 as "No path appears more than once in the grouping prompt."
Read literally that is not implementable while `body` holds real diff content: every
tracked file's hunk begins `diff --git a/<path> b/<path>`, and even the header-only
degradation in D3 keeps those headers. A test asserting one literal occurrence per path
would fail on any fixture with a tracked change.

The ticket's own root-cause section says what it means — "the same file path list **three
times**" — so the criterion is about redundant *inventories*, not about the substring
never recurring. AC2 above is that reading. Flagged for the owner at the design checkpoint
because it restates a ticket criterion.

## Corrections to the initial scoping

Two assumptions recorded at classification proved wrong on inspection, both in the
direction of *less* work. Recorded here so the plan phase does not budget for them:

- **The prompt text is built in one place.** All six providers call
  `super::grouping_user_content(ctx)` (`src/provider/facade.rs:366`) — `openai.rs:178`,
  `ollama.rs:202`, `gemini.rs:148`, `groq.rs:153`, `anthropic.rs:168`. Reshaping
  `GroupingContext` touches three production files (`diff.rs`, `provider/facade.rs`,
  `privacy/facade.rs`) plus six single-fixture test constructors, not six payload builders.
- **The `Provider` trait signature does not need to change.** `diff_budget(&self)` already
  has `&self`, and every provider owns its `self.model`. The budget can consult the model
  without touching the trait. `build_plan` (`src/main.rs:583`) also has `model: &str` in
  scope if a call-site override is ever wanted.

## Architecture

### Affected components

| Component | Change | Description |
|---|---|---|
| `src/diff.rs` | Modified | `GroupingContext` shape, assembly-time budget, untracked render mode, token estimator |
| `src/provider/facade.rs` | Modified | `grouping_user_content` renders one merged inventory; `GROUPING_SYSTEM_PROMPT` rule wording follows |
| `src/privacy/facade.rs` | Modified | `prepare_grouping` scans the merged field instead of two |
| `src/provider/*.rs` (6) | Modified | test `ctx()` fixtures only |
| `src/provider/models.rs` | Modified | the model → context-window table and its lookup |
| `src/main.rs` | Modified | shared pre-flight call site on both the grouping and single-commit paths |
| `src/error.rs` | Modified | the typed over-budget error |

### D1 — Send each path once

Replace `file_list` + `status` with one field:

```rust
pub struct GroupingContext {
    /// JSON array of one object per changed path — every path exactly once,
    /// carrying its porcelain status and, for a rename or copy, the original
    /// path. Replaces the CLO-487 `file_list` + `status` pair, which sent the
    /// same paths twice (CLO-797).
    pub files: String,
    pub stat: String,
    pub body: String,
}
```

Each entry is `{"path": "...", "xy": "..."}`, plus `"old_path": "..."` when
`ChangedFile::orig_path` is `Some` (git sets it for `R`/`C` in either status column —
`src/git.rs:682`). Including the old path closes a gap that predates this task: today's
`status_json` renders only `"XY path"` and drops `orig_path` entirely, even though
`stage_paths()` (`src/git.rs:657`) stages both names. The model is still told to group by
the **new** path, which is what `GROUPING_SYSTEM_PROMPT` already instructs and what plan
validation matches on.

The array **must** be built with `serde_json`, never `format!`. That is what keeps a path
containing a newline, a quote, a backslash or a control character a single well-formed
element — the newline-safety that motivated the CLO-487 arrays in the first place.

Rendered:

```
Changed files (JSON array of {path, xy, old_path?} — group by "path"):
[{"path":"src/diff.rs","xy":" M"},{"path":"docs/new.md","xy":"??"}]
```

Measured effect on the reproduction: ~36K tokens removed outright.

### D2 — Drop the redundant untracked stubs

In `append_untracked`, the past-the-cap branch emits a three-line block per remaining
file. Every one of those paths is already in the inventory with `xy: "??"`, so the block
conveys nothing new. For the grouping caller it is replaced by a single trailer:

```
[content omitted: 463 further untracked files past the read cap — see the file list]
```

`gather_for_files` (per-group message regeneration) keeps today's per-file stubs: it
passes an allow-list and carries no inventory, so those stubs are the only record of
those paths. The two behaviours are selected by an explicit mode, not a bare boolean:

```rust
enum UntrackedRender {
    /// One `--- /dev/null` block per file past the cap (single-commit and
    /// per-group paths, which have no inventory to refer to).
    PerFileStubs,
    /// One trailer naming the count (grouping, where the inventory lists them).
    SummaryTrailer,
}
```

Measured effect: ~39.6K tokens removed.

With D1 and D2 alone the reproduction lands at ~113,200 tokens — inside the 131,072
window. D3-D5 are what stop the class of bug recurring at higher file counts.

### D3 — Bound the assembled prompt, not `body`

`gather_for_grouping` applies `cap_total` to `body` before the other sections exist. The
bound moves after assembly and covers everything the request will carry:

```
budget = window_tokens − completion_reserve − envelope_allowance
spent  = est(rendered grouping_user_content)
```

`envelope_allowance` is **not** a guessed constant. It is
`est(GROUPING_SYSTEM_PROMPT) + est(plan::schema()) + ENVELOPE_SLACK`, where the first two
are computed from the constants already in the binary and `ENVELOPE_SLACK` (1,024 tokens)
covers the provider's JSON request envelope and parameter fields. Computing rather than
hardcoding means the allowance cannot drift when the system prompt is edited.

Sections are not equally compressible, so the bound is applied in a fixed order of
sacrifice, cheapest information first:

1. Shrink `body` by lowering the effective per-file cap and re-truncating.
2. Reduce `body` to header-only sections (`[diff omitted: N bytes]` — the CLO-487
   placeholder already exists).
3. Drop `body` entirely, keeping the inventory + `stat`. This is the ticket's scope
   bullet 5 verbatim: "drop hunks and group on paths + stat alone".
4. If the inventory + `stat` still exceed the budget → D5 error.

The inventory and `stat` are never truncated. A partial path list would break the CLO-487
path-agreement contract: the model would return groups that omit files, and validation
would reject the plan.

**Why step 3 is a degraded mode and not an error** (raised by review as an AC4 conflict):
the ticket asks for exactly this — grouping on paths and stat when hunks do not fit —
and grouping by path is genuinely informative, since the model groups by directory and
file-type affinity as much as by hunk content. The mode is announced, not silent (D6).
AC4's error is the *terminal* rung, step 4, plus the single-commit path's own diff-too-large
case. Both now have one definition and one error type.

### D4 — Derive the budget from the model's context window

This is the one decision the investigation leaves open: gcm holds no context-window
metadata anywhere in `src/`. `static_fallback_models` (`src/provider/models.rs:444`) is a
flat `Vec<String>`; `http.rs` knows windows only as error-string patterns.

Three sources were considered:

| Source | Pro | Con |
|---|---|---|
| **A. Static table keyed by (provider, model)** | No network call, works offline and with no API key, matches the existing model-matching idiom (`groq.rs:171`) | Goes stale; unknown models need a default |
| B. Live from the provider's model-list API | Always current | Groq returns `context_window`, OpenAI does not — not universal; costs an HTTP round-trip every run; fails with no key |
| C. User config knob | Exact for local models | Asks the user for a number they usually do not know; another config surface |

**Chosen: A**, with the lookup rules the review asked for:

1. **Exact `(provider, model)` match** wins. Keying on the provider as well as the model
   matters because the same name can carry different windows on different hosts —
   acutely so for Ollama, where the window is whatever the Modelfile sets.
2. **Longest family-prefix match** second, so `gpt-oss-120b` cannot be matched by a
   shorter, smaller-context family entry. Longest-wins makes substring collisions
   deterministic instead of declaration-order dependent.
3. **`DEFAULT_WINDOW` = 32,768 tokens** for anything unmatched, *and* a warning naming
   the model and the assumed window, so a user on an unlisted model learns why gcm
   degraded. 32,768 is a deliberate under-estimate: degrading a prompt costs grouping
   quality, exceeding a window costs the whole call.
4. **`GCM_CONTEXT_WINDOW_TOKENS`** overrides both, matching the existing
   `GCM_DIFF_TOTAL_BYTES` override shape. Validation: a positive integer within a sane
   bound; anything else is a config error naming the variable, not a silent fallback.

Seeding the table: only values that can be cited go in it. `openai/gpt-oss-120b` =
131,072 is established by the investigation's own measurement against Groq. Every other
entry is filled during implementation from that provider's published limit, with the
source named in a comment beside it. No window is invented.

The estimator stays characters-per-token. Measured here: 2.31 for path JSON, 2.91 for
diff body. The review is right that those come from ASCII-heavy content and are not a
universal bound — CJK, emoji and escaped control characters can push the true ratio well
below 2.3, which would make the estimate *under*-count. So:

- divisor **2.0**, not 2.3, for a margin under the ASCII measurements;
- text that is not predominantly ASCII is estimated at **1.0 char/token**, which is at or
  below any real tokenizer's worst case;
- and the provider-400 classifier stays as the second line of defence (below).

`DiffBudget` keeps `per_file_bytes` and gains the token budget; `total_bytes` becomes
derived rather than a per-provider constant. `GCM_DIFF_TOTAL_BYTES` keeps working as an
explicit byte ceiling applied on top, so no existing override breaks.

### D5 — One pre-flight check, shared by both call paths

Resolves the draft's Q3. The check belongs in neither `gather_for_grouping` (which would
make the gatherer a policy layer) nor `build_plan` alone (which does not own the
single-commit path). It is a small shared service called from the orchestration layer
before **every** provider call that carries a diff:

```rust
pub struct PromptBudget {
    pub window_tokens: usize,
    pub completion_reserve: usize,
    pub envelope_allowance: usize,
}

/// Err when the rendered prompt cannot fit. Makes no network call.
pub fn ensure_prompt_fits(rendered: &str, budget: &PromptBudget) -> Result<(), GcmError>;
```

Call sites: the grouping plan call, the single-commit message call, and the fallback
message call. `diff.rs` stays a gatherer, `build_plan` keeps owning the fallback decision,
and both paths raise the same typed error with the same advice.

`completion_reserve` is a safety parameter, not an open question (the draft wrongly
deferred it). Today's plan schema requires `groups[].files`, so the model echoes every
path back — the investigation measured ~36K output tokens for 515 files, which is a real
fraction of the window:

```
completion_reserve = FIXED_OUTPUT_OVERHEAD (1,024)      // summaries, commit message, JSON scaffolding
                   + reasoning_allowance                // 0 where reasoning is suppressed (groq.rs:171), else provider-specific
                   + min(provider_max_output_tokens, est_path_echo)

est_path_echo = est(files inventory) * 1.15             // the plan re-emits every path plus per-group scaffolding
```

Deriving `est_path_echo` from the serialized inventory rather than from the file count is
deliberate: 100 deep paths and 100 short ones produce very different completions.

This exposes a second failure mode the draft missed. A prompt can fit the window while the
*required completion* exceeds the provider's output cap — Anthropic hardcodes
`max_tokens: 4096` (`src/provider/anthropic.rs:143,165`), so a 515-file plan cannot be
returned by Anthropic at all, whatever the window. When
`est_path_echo > provider_max_output_tokens`, gcm says so and falls back, rather than
sending a request whose answer cannot fit. Raising Anthropic's cap and shrinking the
output are CLO-801's business; naming the failure is this task's.

When the terminal rung is reached, `build_plan` fails **before** the call with an error
naming the measurement and the real remedies (a larger-window model, `--all` for a single
commit, or committing a subset), and falls back to single-commit through the existing
`run_fallback` path with that reason announced. CLO-798 already built this vocabulary in
`src/provider/diagnostics.rs`, including the grouping-specific note that staging a subset
does not help because `gather_for_grouping` reads the whole worktree; the new error reuses
that advice rather than writing a second copy.

**The estimator is not trusted as the only defence.** If it under-counts anyway, the
provider still returns a context-window rejection, and CLO-798's classifier still
translates it into the same actionable advice instead of a raw HTTP failure. The
pre-flight makes the common case explicable; the classifier remains the backstop.

**Two estimates, one per decision (owner decision 2026-10-29).** Shaping and refusing
need different errors. The grouping ladder and the grouping pre-flight use the
pessimistic estimate (2 bytes per token, 1 character per token for mostly non-ASCII
text): an over-count there only sends a smaller prompt. It is not a strict upper bound:
emoji cost about 3 tokens per character against the o200k vocabulary, so emoji-heavy
text is undercounted, and the provider's context-window error remains the backstop
there (Codex validation, finding 1). A message is never shrunk, so
the message pre-flight for `--all`, the fallback and per-group regeneration uses a
generous estimate of 4 bytes per token instead, and refuses only when even that
overflows. The measured diff tokenized at about 2.8 bytes per token, and the live run
showed the 2-byte estimate refusing a 281 KB `--all` message (~144K estimated) that Groq
accepted. The completion and envelope reserves are unchanged.

The generous estimate counts non-ASCII text by bytes too. Cyrillic (2 bytes per
character) and CJK (3) are therefore undercounted, deliberately: an undercount ends in
the provider's context-window error, which CLO-798 already turns into the same advice.
It remains a heuristic. Text that tokenizes above 4 bytes per token, such as long runs of
whitespace or repeated words, can be refused although the model would accept it;
`GCM_CONTEXT_WINDOW_TOKENS` is the escape.

### D6 — Degradation is announced, never silent

Every rung of the D3 ladder, and every use of `DEFAULT_WINDOW`, emits a user-visible line
through the CLO-798 diagnostics vocabulary. Without it, a poor grouping caused by
aggressive truncation is indistinguishable from a poor model:

```
warning: grouping prompt reduced to fit the model context window
  estimated prompt: 118,432 tokens   window: 131,072   model: openai/gpt-oss-120b
  412 of 515 files reduced to headers
```

and, on an unlisted model:

```
warning: context window unknown for ollama:qwen3.8:27b-mlx; assuming 32,768 tokens
  set GCM_CONTEXT_WINDOW_TOKENS to the real window
```

`--json` keeps stdout to exactly one envelope; these go to stderr, matching CLO-798.

## Test plan

Unit, in `src/diff.rs` and `src/provider/models.rs`:

1. **One inventory (AC2)**: the rendered prompt contains exactly one inventory, and no
   path has a stub duplicating it. Counts occurrences in the inventory and in generated
   omission stubs — not literal substring occurrences across the whole prompt, which
   `diff --git` headers legitimately produce.
2. **Assembled-prompt budget (AC3)**: a fixture that overflows asserts the *rendered*
   `grouping_user_content` plus the envelope allowance is within budget. This is the test
   whose absence let the bug ship.
3. **Sacrifice order**: body degrades through all three rungs before the inventory is
   touched; the inventory and `stat` are never truncated.
4. **Untracked trailer**: past the read cap, one trailer replaces the per-file stubs and
   its count is right.
5. **`gather_for_files` unchanged**: still emits per-file stubs (`UntrackedRender::PerFileStubs`).
6. **Rename and copy**: an `R`/`C` entry carries both `path` and `old_path`, and both are
   still staged by `stage_paths()`.
7. **Path encoding**: newline, double quote, backslash and non-ASCII paths each survive as
   one well-formed JSON element.
8. **Window lookup**: exact `(provider, model)` beats family prefix; longest prefix beats
   shorter on a deliberate collision fixture; unknown model yields `DEFAULT_WINDOW` and
   warns.
9. **Env override validation**: empty, non-numeric, zero and implausibly large
   `GCM_CONTEXT_WINDOW_TOKENS` each produce a config error naming the variable; a valid
   value overrides both table and default.
10. **Estimator margin**: non-ASCII text is estimated at no more than 1 char/token.
11. **Output-cap failure**: `est_path_echo` above a provider's `max_tokens` is reported as
    its own failure, distinct from a window overflow.

Integration:

12. **The 515-file reproduction** from the investigation, against the local capture server,
    asserts the captured payload is under 131,072 estimated tokens (AC1). Deterministic,
    no API key.
13. **Fallback matrix (AC4)**: grouping over budget with single-commit fitting → fallback
    succeeds; both over budget → one clear gcm error, never a 400.
14. **No network on pre-flight failure**: the capture server records zero requests.
15. **Degradation is visible**: the warning appears on stderr and stdout stays one
    envelope under `--json`.

`make check` (fmt, clippy `-D warnings`, tests). `scripts/check-public-surface.sh` should
stay green untouched: `GroupingContext` lives in the binary-only `diff` module (see Q1), so
this change does not move the library surface.

## Resolved questions

- [x] **Q1 — `GroupingContext` is not public.** `mod diff;` is declared only in
      `src/main.rs:5`, so the module is binary-only, and `scripts/check-public-surface.sh`
      is a denylist that already forbids `trait.Provider` for the same reason. Reshaping
      the struct needs no deprecation aliases and no surface-file update.
- [x] **Q2 — `completion_reserve` is defined**, not deferred: see D5. It accounts for the
      path-echoing output the current schema forces, reasoning tokens, and the provider's
      own output cap.
- [x] **Q3 — the pre-flight lives in a shared checker** called from the orchestration
      layer, not in `diff.rs` and not only in `build_plan`: see D5.

## Owner decisions (2026-09-17)

Both settled at the design checkpoint; no open questions remain.

- [x] **AC2 restatement approved.** The implementation targets the ticket's intent — one
      path inventory, no redundant list or stub copy — rather than its literal wording,
      which no prompt containing a diff can satisfy. The literal reading (which would
      force `body` to reference files by index) was considered and declined.
- [x] **`DEFAULT_WINDOW` = 32,768 approved** for unlisted models, with the warning in D6.
      Deliberately pessimistic: it is the value Ollama users hit, since gcm cannot read a
      Modelfile's `num_ctx`, and degrading a prompt is cheaper than losing the call.

## References

- [Linear CLO-797](https://linear.app/cloud-ai/issue/CLO-797)
- [Investigation, with measurements and the reproduction](../investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md)
- [Codex design review](../reviews/clo-797-review-ollama.md) — NEEDS_REVISION, 6 P0 items, all applied
- [CLO-487 semantic grouping spec](../specs/2026-06-20-clo-487-semantic-grouping.md) — path-agreement contract, FR-15
- [CLO-489 provider trait spec](../specs/2026-06-21-clo-489-provider-trait.md) — where `STANDARD_TOTAL` came from
- [CLO-798 provider call visibility spec](../specs/2026-09-16-clo-798-provider-call-visibility.md) — diagnostics and the 400 classifier this builds on
