# Plan: CLO-797 Bound the grouping prompt to the model's context window

## Context
- Design: `docs/designs/clo-797-grouping-prompt-budget.md` (Finalized 2026-09-17)
- Review: `docs/reviews/clo-797-review-ollama.md` (NEEDS_REVISION → 13/13 applied)
- Investigation: `docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md`
- Linear: https://linear.app/cloud-ai/issue/CLO-797

Settled at the design checkpoint and unchanged here:
- **AC2** means the prompt carries exactly one path inventory. Paths inside real
  `diff --git` headers are diff content, not a second copy.
- **`DEFAULT_WINDOW = 32_768`** for any model the table does not list.

Prior lessons consulted (`.pi/lessons/`):
- `clo-598-consumer-check-lessons.md § L1` - a guard that asserts a bound must be **proven
  to fail** before it is trusted. ST5's budget regression test is exactly such a guard, so
  it is written against the pre-fix assembly first and shown red (see ST5 acceptance).
- `clo-598-consumer-check-lessons.md § L2` - a test must execute the branch it names. The
  ladder tests assert on *which rung fired*, not merely that the output got smaller.
- `clo-493-...-lessons.md § L1` - `--json` stdout stays exactly one envelope; every D6
  warning goes to stderr.
- `clo-594-library-boundary-lessons.md` - `diff` is binary-only (`mod diff;` in
  `src/main.rs:5`); nothing here may leak into `src/lib.rs`. The `Provider` trait is part
  of the locked library surface (`tests/library_provider_api.rs`), so no trait method is
  added: output caps and reasoning allowances live in a binary-side lookup.

### Two budget kinds

There are two kinds of provider call that carry a diff, and they get separate budgets:

| | Grouping (plan call) | Message (single-commit, fallback, cached group-1 regeneration) |
|---|---|---|
| Call site | `build_plan` (`src/main.rs:583`) | `single_commit_path` (`src/main.rs:848`, also reached from `run_fallback`); `commit_first_group` regeneration (`src/main.rs:663`) |
| Gatherer | `gather_for_grouping` - window-derived token budget plus the D3 ladder (ST4) | `gather_for_changed` / `gather_for_files` - unchanged: `DiffBudget` byte caps and truncation exactly as today |
| System/envelope allowance | `est(GROUPING_SYSTEM_PROMPT) + est(plan::schema()) + ENVELOPE_SLACK` | `est(SYSTEM_PROMPT) + ENVELOPE_SLACK` |
| Completion reserve | `FIXED_OUTPUT_OVERHEAD + reasoning_allowance + min(plan_output_cap, est_path_echo)` | `message_output_cap.unwrap_or(FIXED_OUTPUT_OVERHEAD) + reasoning_allowance` |
| Path-echo check | yes - `est_path_echo > plan_output_cap` rejects the grouping call | **no** - a message does not echo paths |
| On rejection | announced fallback to the message path (`BuildError::Fallback`) | gcm error envelope; zero HTTP requests |

`ENVELOPE_SLACK = 1_024` and `FIXED_OUTPUT_OVERHEAD = 1_024` tokens, as in the design.
`est_path_echo = estimate_tokens(files) * 1.15`, from the serialized ST1 inventory.

Ordering: ST1 and ST2 are independent. ST3 needs ST1 (the path-echo estimate reads the
inventory). ST4 needs ST3's grouping budget. ST5 needs ST3 and ST4. ST6 needs ST4 and ST5.
ST7 needs everything.

## Sub-tasks

### ST1 Merge the file list and status into one inventory
**Files:** `src/diff.rs`, `src/provider/facade.rs`, `src/privacy/facade.rs`, six `ctx()` test fixtures
**Acceptance:** `cargo test diff::tests::inventory` passes; `cargo test` green across all six providers
**Estimate:** M

- Replace `GroupingContext.file_list` + `.status` with `files: String`.
- New `files_json(changed: &[ChangedFile]) -> String`: one object per path,
  `{"path", "xy"}` plus `"old_path"` when `ChangedFile::orig_path` is `Some`. Built with
  `serde_json` (a `Vec` of a `#[derive(Serialize)]` struct), never `format!`.
- Delete `file_list_json` and `status_json`.
- `grouping_user_content` (`facade.rs:366`) renders one labelled block:
  `Changed files (JSON array of {path, xy, old_path?} — group by "path"):`.
- `GROUPING_SYSTEM_PROMPT`: the rule "Every file from the file list must appear in exactly
  one group" keeps its meaning; reword to name the inventory, and keep the existing
  "For renamed files, use the NEW path" rule, which now matches `path` explicitly.
- `privacy::facade::prepare_grouping` scans `files` instead of the two fields.
- Tests: rename/copy carries both names and both still appear in `stage_paths()`; newline,
  double-quote, backslash and non-ASCII paths each survive as one JSON element.

### ST2 Collapse the past-the-cap untracked stubs
**Files:** `src/diff.rs`
**Acceptance:** `cargo test diff::tests::untracked_trailer` and `diff::tests::gather_for_files_keeps_stubs` pass
**Estimate:** S

- Add `enum UntrackedRender { PerFileStubs, SummaryTrailer }`; `append_untracked` takes it
  instead of inferring from the allow-list.
- `gather_for_grouping` passes `SummaryTrailer`: count the files skipped past the cap and
  emit one trailer, `[content omitted: N further untracked files past the read cap — see
  the file list]`.
- `gather` and `gather_for_files` pass `PerFileStubs` - unchanged behaviour.
- Tests: with 60 untracked files and `MAX_UNTRACKED_FILES = 50`, the grouping body holds
  one trailer reading 10 and zero `--- /dev/null` stub headers for the skipped files;
  `gather_for_files` on the same fixture still emits per-file stubs.

### ST3 Token estimator, window table, and the two budget kinds
**Files:** `src/provider/models.rs`, `src/diff.rs` (or a small binary-only `src/budget.rs`)
**Acceptance:** `cargo test models::tests::context_window`, `diff::tests::estimate` and `budget::tests::kinds` pass
**Estimate:** M

- `fn estimate_tokens(text: &str) -> usize`: divisor 2.0 for predominantly-ASCII text,
  1.0 for text that is not (decide per call on a cheap scan, e.g. non-ASCII bytes over a
  small fraction of the whole). Erring low on chars/token over-counts tokens, which is the
  safe direction.
- `fn context_window(provider: ProviderId, model: &str) -> WindowSource`, returning the
  window plus whether it came from the table, the default, or the env override:
  1. exact `(provider, model)`;
  2. longest matching family prefix (longest wins, so a shorter entry cannot shadow a
     larger-context sibling);
  3. `DEFAULT_WINDOW = 32_768`.
- `GCM_CONTEXT_WINDOW_TOKENS` overrides all three; parse as a positive integer within a
  sane bound, and return a config error naming the variable on empty, non-numeric, zero or
  implausibly large input (do **not** silently fall through, unlike `env_usize`).
- Seed the table only with citable values: `(Groq, "openai/gpt-oss-120b") = 131_072` is
  established by the investigation. Fill the rest from each provider's published limit
  during this sub-task, naming the source in a comment beside each entry. Invent no window.
- Binary-side per-provider output facts, no `Provider` trait change: `plan_output_cap`
  and `message_output_cap` (Anthropic: `Some(4096)` and `Some(1024)`, read from named
  `pub(crate)` constants that `anthropic.rs:143,165,183` then uses, so the payload and the
  budget cannot drift; every other provider: `None`), and `reasoning_allowance` (0 where
  `groq.rs:171` suppresses reasoning, otherwise a provider-specific value with its source
  in a comment).
- `struct PromptBudget { kind, window_tokens, envelope_allowance, completion_reserve }`
  with two constructors, so every budget input exists before ST4 applies its ladder:
  - `PromptBudget::grouping(window, provider, files_json) -> Result<Self, GroupingRejected>` -
    grouping allowance and reserve from the table above. Returns `GroupingRejected::OutputCap`
    when `est_path_echo > plan_output_cap`, before any body is gathered.
  - `PromptBudget::message(window, provider) -> Self` - message allowance and reserve; no
    path-echo term and no output-cap rejection.
  - `fn available(&self) -> usize` = `window − envelope_allowance − completion_reserve`
    (saturating).
- Tests: exact beats prefix on a deliberate collision fixture; longest prefix beats
  shorter; unknown model returns `DEFAULT_WINDOW` and reports the default as its source;
  each invalid env value yields a config error naming `GCM_CONTEXT_WINDOW_TOKENS`;
  `grouping` and `message` for the same provider produce different allowances and
  reserves; a path-echo estimate over Anthropic's 4096 rejects `grouping` while `message`
  for the same inventory size succeeds; Groq with reasoning suppressed gets 0 allowance.

### ST4 Budget the assembled grouping prompt, with the sacrifice ladder
**Files:** `src/diff.rs`, `src/main.rs` (pass the grouping budget into the gatherer)
**Acceptance:** `cargo test diff::tests::ladder` passes with each rung asserted by name; `diff::tests::message_gathering_unchanged` passes
**Estimate:** M

- **Message gathering is out of scope.** `DiffBudget` (`STANDARD_TOTAL`, OpenAI's
  `256_000`, `STANDARD_PER_FILE`, `GCM_DIFF_TOTAL_BYTES` / `GCM_DIFF_PER_FILE_BYTES`) and
  `cap_total` stay exactly as they are for `gather`, `gather_for_files` and
  `gather_for_changed`. Nothing in this sub-task changes what a message prompt contains.
- `gather_for_grouping(repo, changed, diff_budget, grouping: &PromptBudget)` derives its
  own bound from `grouping.available()`. `diff_budget.per_file_bytes` stays the rung-0
  per-file cap, and `diff_budget.total_bytes` stays a body ceiling, so grouping is never
  looser than today. The ceiling is enforced by the ladder, not by `cap_total`'s tail-chop,
  so no file is severed mid-hunk.
- Ladder over the rendered `grouping_user_content`: shrink `body` via a lowered per-file
  cap → headers only → drop `body` → signal "cannot fit" to the caller (ST5 owns the error).
- The inventory and `stat` are never truncated.
- Tests, each asserting **which rung fired** (per lesson L2), not just that output shrank:
  a fixture that fits untouched; one that needs rung 1; one that needs rung 2; one that
  needs rung 3; one where the inventory alone overflows, which reports "cannot fit"; one
  that fits the token budget but exceeds `total_bytes`, which takes rung 1 rather than a
  tail-chop. `message_gathering_unchanged`: on a fixed fixture, `gather_for_changed` and
  `gather_for_files` output is byte-identical to the pre-change output (captured before
  editing `diff.rs`).

### ST5 One shared pre-flight check, the typed error, and four call sites
**Files:** `src/diff.rs` (or `src/budget.rs`), `src/error.rs`, `src/main.rs`
**Acceptance:** `cargo test budget::tests` passes, **and** the regression guard is first proven red - see below
**Estimate:** M

- `fn ensure_prompt_fits(rendered: &str, budget: &PromptBudget) -> Result<(), GcmError>`.
  Pure and synchronous; makes no network call. `rendered` is the user content the
  provider will send (`grouping_user_content` or `message_user_content`); the system
  prompt and envelope are already inside `budget.envelope_allowance`.
- New `GcmError` variant carrying the estimate, the window, the budget kind and the call
  site, with three causes: grouping prompt cannot fit (terminal ladder rung), grouping
  output exceeds the plan output cap, message prompt cannot fit. Its message reuses the
  CLO-798 advice in `src/provider/diagnostics.rs` rather than writing a second copy.
- Call sites, each **before** its provider call, so a rejection issues zero HTTP requests:
  1. `build_plan` (`src/main.rs:583`) - grouping budget. Both grouping causes become
     `BuildError::Fallback` with the reason, so `run_fallback` runs the message path
     under the message budget. `diff.rs` stays a gatherer; `build_plan` keeps owning the
     fallback decision.
  2. `single_commit_path` (`src/main.rs:848`) - message budget, for both
     `Operation::SingleMessage` and `Operation::FallbackMessage`.
  3. `commit_first_group` cached-plan regeneration (`src/main.rs:663`) - message budget,
     `Operation::GroupMessage`. A rejection returns the error envelope in
     `MODE_GROUPED` and leaves the cache and the index untouched.
- **Prove the guard fails (lesson L1).** Before wiring the fix, run the new
  assembled-prompt budget test against the pre-fix assembly (the 515-file fixture with
  `file_list` + `status` + stubs restored) and confirm it exits non-zero. A budget assertion
  that has never been red is not evidence. Record the red run in the PR body.

### ST6 Announce every degradation
**Files:** `src/main.rs`, `src/provider/diagnostics.rs`, `src/ui.rs`
**Acceptance:** `cargo test observability` passes; `cargo test --test observability` covers stderr routing
**Estimate:** S

- Emit a warning on each ladder rung that fires, naming the estimate, the window, the model
  and how many files were reduced.
- Emit a warning whenever `DEFAULT_WINDOW` is used, naming the model and pointing at
  `GCM_CONTEXT_WINDOW_TOKENS`.
- A grouping-output rejection is announced through the existing `run_fallback` line,
  with the ST5 reason naming the estimated echo and the provider's output cap.
- All of it on stderr. Under `--json`, stdout stays exactly one envelope (lesson: CLO-493
  L1); reuse the CLO-798 routing rather than adding a second mechanism.

### ST7 Integration coverage
**Files:** `tests/` (new `grouping_budget.rs`), reusing `docs/investigations/clo-797-assets/capture_server.py` or an in-test Rust listener with the same contract
**Acceptance:** `cargo test --test grouping_budget` passes with no network access
**Estimate:** M

The capture server records every request and, per test, either returns HTTP 400 (as the
investigation's server does) or a canned success body in the provider's wire format.

- **AC1**: the 515-file reproduction against the local capture server asserts the captured
  payload is under 131,072 estimated tokens. Deterministic, no API key
  (`GCM_GROQ_BASE_URL` + `GCM_RETRY_MAX=0`, as the investigation documents).
- **AC2**: one path inventory - count occurrences in the inventory and in generated
  omission stubs, *not* literal substring occurrences across the whole prompt, which real
  `diff --git` headers legitimately produce.
- **AC4 fallback matrix**:
  - grouping over budget with the message fitting → fallback succeeds;
  - both over budget → one clear gcm error and never a provider 400.
- **Grouping-output rejection, then a successful message fallback**: Anthropic via
  `GCM_ANTHROPIC_BASE_URL`, an inventory whose path echo exceeds 4096 while the message
  prompt fits. The server records exactly one request, it is a message payload (no
  `tools`, `max_tokens: 1024`), the canned success is committed, and the envelope status
  is `fallback`.
- **Zero requests on pre-flight rejection**, one case per call site, each asserting the
  server recorded nothing and the error envelope names the cause:
  - grouping terminal rung with the fallback message also over budget;
  - `--all` single-commit message over budget;
  - **cached group-1 regeneration**: seed the plan cache (format in `src/cache.rs`) with an
    advanced plan whose group-1 message is null, set a small `GCM_CONTEXT_WINDOW_TOKENS`,
    run gcm, and assert zero requests, the `MODE_GROUPED` error, and an unchanged cache
    file and index.
- **Degradation is visible**: the warning lands on stderr while `--json` stdout stays one
  envelope.

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- `scripts/check-public-surface.sh` green **untouched** - `GroupingContext` and
  `PromptBudget` are binary-only and the `Provider` trait does not change, so this
  change must not move the library surface.
- `cargo build --no-default-features --lib` still green.
- The ST5 red-run evidence quoted in the PR body.

## Risks
- **Estimator under-counts on unusual content.** Mitigated by the 2.0 divisor, the 1.0
  non-ASCII rule, and by keeping the CLO-798 provider-400 classifier as the backstop: if
  the estimate is wrong the user still gets context-window advice, not a raw HTTP error.
- **`DEFAULT_WINDOW` is wrong for a given local model.** 32,768 is deliberately
  pessimistic; the D6 warning names the assumption and the override. Ollama windows come
  from the Modelfile and gcm cannot read them.
- **The message pre-flight can reject a prompt that today's provider would accept.**
  Message gathering is unchanged, so this happens only when the estimate says the diff
  exceeds the window - the case that today ends in a provider 400 anyway. A wrong table
  entry is fixed with `GCM_CONTEXT_WINDOW_TOKENS`.
- **Rung 3 (paths + stat only) may group worse.** It is the ticket's own scope bullet 5 and
  it is announced, not silent. The alternative - refusing to group at all - loses the
  feature entirely on exactly the trees where it matters most.
- **Table staleness.** Unlisted models degrade to the warned default rather than failing,
  so a stale table costs grouping quality, never a broken run.
- **Scope pressure from CLO-801.** The grouping completion reserve exists because today's
  schema echoes every path. When CLO-801 lands, `est_path_echo` should shrink
  dramatically; the reserve is one formula in `PromptBudget::grouping`, so that change is
  a one-line edit.
