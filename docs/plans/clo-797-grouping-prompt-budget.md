# Plan: CLO-797 Bound the grouping prompt to the model's context window

## Context
- Design: `docs/designs/clo-797-grouping-prompt-budget.md` (Finalized 2026-09-17)
- Review: `docs/reviews/clo-797-review-ollama.md` (NEEDS_REVISION → 13/13 applied)
- Investigation: `docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md`
- Linear: https://linear.app/cloud-ai/issue/CLO-797

Prior lessons consulted (`.pi/lessons/`):
- `clo-598-consumer-check-lessons.md § L1` - a guard that asserts a bound must be **proven
  to fail** before it is trusted. ST5's budget regression test is exactly such a guard, so
  it is written against the pre-fix assembly first and shown red (see ST5 acceptance).
- `clo-598-consumer-check-lessons.md § L2` - a test must execute the branch it names. The
  ladder tests assert on *which rung fired*, not merely that the output got smaller.
- `clo-493-...-lessons.md § L1` - `--json` stdout stays exactly one envelope; every D6
  warning goes to stderr.
- `clo-594-library-boundary-lessons.md` - `diff` is binary-only (`mod diff;` in
  `src/main.rs:5`); nothing here may leak into `src/lib.rs`.

Ordering: ST1-ST2 are independent of ST3. ST4 depends on ST3 (needs the estimator), ST5
depends on ST3+ST4, ST6 depends on ST4+ST5, ST7 depends on everything.

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

### ST3 Token estimator and the model → context-window table
**Files:** `src/provider/models.rs`, `src/diff.rs`
**Acceptance:** `cargo test models::tests::context_window` and `diff::tests::estimate` pass
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
- Tests: exact beats prefix on a deliberate collision fixture; longest prefix beats
  shorter; unknown model returns `DEFAULT_WINDOW` and reports the default as its source;
  each invalid env value yields a config error naming `GCM_CONTEXT_WINDOW_TOKENS`.

### ST4 Budget the assembled prompt, with the sacrifice ladder
**Files:** `src/diff.rs`
**Acceptance:** `cargo test diff::tests::ladder` passes; each rung asserted by name
**Estimate:** M

- `DiffBudget` gains the token budget; `total_bytes` becomes derived. `GCM_DIFF_TOTAL_BYTES`
  still applies as an explicit byte ceiling on top, so existing overrides keep working.
- `envelope_allowance` is computed, not hardcoded:
  `estimate_tokens(GROUPING_SYSTEM_PROMPT) + estimate_tokens(&plan::schema().to_string())
  + ENVELOPE_SLACK (1_024)`.
- `gather_for_grouping` drops the `cap_total(&mut body, …)` call and instead applies the
  ladder to the rendered prompt: shrink `body` via a lowered per-file cap → headers only →
  drop `body` → signal "cannot fit" to the caller (ST5 owns the error).
- The inventory and `stat` are never truncated.
- Tests, each asserting **which rung fired** (per lesson L2), not just that output shrank:
  a fixture that fits untouched; one that needs rung 1; one that needs rung 2; one that
  needs rung 3; one where the inventory alone overflows, which reports "cannot fit".

### ST5 One shared pre-flight check and the typed error
**Files:** `src/diff.rs` (or a small `src/budget.rs`), `src/error.rs`, `src/main.rs`
**Acceptance:** `cargo test budget::tests` passes, **and** the regression guard is first proven red - see below
**Estimate:** M

- `struct PromptBudget { window_tokens, completion_reserve, envelope_allowance }` and
  `fn ensure_prompt_fits(rendered: &str, budget: &PromptBudget) -> Result<(), GcmError>`.
  Pure and synchronous; makes no network call.
- `completion_reserve = FIXED_OUTPUT_OVERHEAD (1_024) + reasoning_allowance +
  min(provider_max_output_tokens, est_path_echo)`, with
  `est_path_echo = estimate_tokens(files) * 1.15` - derived from the serialized inventory,
  not the file count, because path length varies enormously.
- Separate failure when `est_path_echo > provider_max_output_tokens`: the answer cannot fit
  even if the prompt does. Anthropic hardcodes `max_tokens: 4096`
  (`src/provider/anthropic.rs:143,165`), so this is reachable today.
- New `GcmError` variant carrying the estimate, the window and the call site; its message
  reuses the CLO-798 advice in `src/provider/diagnostics.rs` rather than writing a second
  copy.
- Call it from `build_plan` (`src/main.rs:583`), the single-commit path
  (`src/main.rs:848`) and the fallback message call - **before** each provider call, so a
  failure issues zero HTTP requests. `diff.rs` stays a gatherer; `build_plan` keeps owning
  the fallback decision.
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
- All of it on stderr. Under `--json`, stdout stays exactly one envelope (lesson: CLO-493
  L1); reuse the CLO-798 routing rather than adding a second mechanism.

### ST7 Integration coverage
**Files:** `tests/` (new `grouping_budget.rs`), reusing `docs/investigations/clo-797-assets/capture_server.py`
**Acceptance:** `cargo test --test grouping_budget` passes with no network access
**Estimate:** M

- **AC1**: the 515-file reproduction against the local capture server asserts the captured
  payload is under 131,072 estimated tokens. Deterministic, no API key
  (`GCM_GROQ_BASE_URL` + `GCM_RETRY_MAX=0`, as the investigation documents).
- **AC2**: no path is duplicated - count occurrences in the inventory and in generated
  omission stubs, *not* literal substring occurrences across the whole prompt, which real
  `diff --git` headers legitimately produce.
- **AC4 fallback matrix**: grouping over budget with single-commit fitting → fallback
  succeeds; both over budget → one clear gcm error and never a provider 400.
- **No network on pre-flight failure**: the capture server records zero requests.
- **Degradation is visible**: the warning lands on stderr while `--json` stdout stays one
  envelope.

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- `scripts/check-public-surface.sh` green **untouched** - `GroupingContext` is binary-only,
  so this change must not move the library surface.
- `cargo build --no-default-features --lib` still green.
- The ST5 red-run evidence quoted in the PR body.

## Risks
- **Estimator under-counts on unusual content.** Mitigated by the 2.0 divisor, the 1.0
  non-ASCII rule, and by keeping the CLO-798 provider-400 classifier as the backstop: if
  the estimate is wrong the user still gets context-window advice, not a raw HTTP error.
- **`DEFAULT_WINDOW` is wrong for a given local model.** 32,768 is deliberately
  pessimistic; the D6 warning names the assumption and the override. Ollama windows come
  from the Modelfile and gcm cannot read them.
- **Rung 3 (paths + stat only) may group worse.** It is the ticket's own scope bullet 5 and
  it is announced, not silent. The alternative - refusing to group at all - loses the
  feature entirely on exactly the trees where it matters most.
- **Table staleness.** Unlisted models degrade to the warned default rather than failing,
  so a stale table costs grouping quality, never a broken run.
- **Scope pressure from CLO-801.** The completion reserve exists because today's schema
  echoes every path. When CLO-801 lands, `est_path_echo` should shrink dramatically; the
  reserve is written as one formula in one place so that change is a one-line edit.
