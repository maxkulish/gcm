# Spec: Return group assignments by index so the grouping plan stops echoing every changed path

**Created**: 2026-11-21
**Linear**: [CLO-801](https://linear.app/cloud-ai/issue/CLO-801)
**Estimated scope**: M (14 code and test files, most provider changes are one-line; 5 sub-tasks; the live checks in sub-task 5 need credentials and are outside the implementation estimate)

## 1. Problem Statement

`gcm` asks the model for a grouping plan in which every group lists its files as path strings
(`groups[].files: [string]`). The model must therefore write every changed path back. On a
515-file working tree (commit `f2e2ad0` of the investigations repository, which is at
`/Users/mk/Code/investigations` on this machine; the investigation doc names `~/Work/investigations`) that reply is about
36K output tokens. Measured with `gemini-3.1-flash-lite`, the grouping call timed out at the
default 60s and needed 85.5s with `GCM_HTTP_TIMEOUT_SECS=300`, while the single-commit message
call took about 1.9s (`docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md`,
sections "The Gemini stall" and "Why grouping takes 85s when the message call takes 1.9s").
Anthropic cannot produce such a reply at all, because the plan call sends
`max_tokens: PLAN_MAX_TOKENS` (4096, `src/provider/anthropic.rs:142`). Since CLO-797, gcm refuses
the call in advance: `PromptBudget::grouping` (`src/budget.rs:427`) estimates the path echo as
`estimate_tokens(files_json) * PATH_ECHO_FACTOR` (`src/budget.rs:29`) and returns
`GroupingRejected::OutputCap` when it exceeds the plan output cap. Either way, a large change set
falls back to one commit.

gcm already sends the authoritative inventory: `diff::files_json` (`src/diff.rs:24`) renders the
changed files as a JSON array of `{path, xy, old_path?}`, and `grouping_user_content`
(`src/provider/facade.rs:383`) puts it in the prompt. The model can refer to an inventory entry by
a small integer instead of by its path. That makes the reply size independent of path length.

The reply shape is stated in five places that must change together:

| Surface | Location | Used by |
|---|---|---|
| `plan::schema()` | `src/plan.rs:310` | Groq, OpenAI, Ollama `format`, Anthropic tool `input_schema` (`src/provider/anthropic.rs:179`) |
| `plan::gemini_schema()` | `src/plan.rs:339` | Gemini, Vertex |
| `GROUPING_SYSTEM_PROMPT` | `src/provider/facade.rs:355` | every provider; Ollama cloud passthrough models ignore `format` and rely on it (CLO-517) |
| `grouping_user_content` header | `src/provider/facade.rs:383` | every provider (`group by "path"`) |
| grouping output reserve | `src/budget.rs:26-29`, `src/budget.rs:423-447`, message text at `src/budget.rs:368-372` | `main::build_plan` (`src/main.rs:625`) |

Who is affected: every user who commits a large change set in grouped mode. What triggers it: a
change set whose path echo exceeds the plan output cap or takes longer than the HTTP timeout to
generate. Why it matters: the user loses the grouping and gets one commit for unrelated changes.
gcm reports the fallback on stderr, but the user cannot get the grouping back.

Facts the implementer needs:

- `Group.files: Vec<String>` (`src/plan.rs:21`) is the type the plan cache persists and that
  `validate` / `validate_cached` / `validate_partition` (`src/plan.rs:379-440`) check. It stays a
  list of paths.
- `privacy.prepare_grouping` (`src/privacy/facade.rs:34`) runs the secret scanner over the
  inventory text, so the inventory the model sees can contain redacted paths. Indices must be
  mapped against the real `changed` slice in `main::build_plan`, never against `ctx.files`.
- Every provider's `generate_plan` ends in `crate::plan::parse_defensive(&json)` (groq.rs:73,
  openai.rs:87, gemini.rs:84, vertex.rs:161, ollama.rs:111, anthropic.rs:90). Anthropic also has
  two direct `serde_json::from_*::<Plan>` shortcuts (`src/provider/anthropic.rs:87` and `:257`)
  that bypass `parse_defensive`. The second one re-serializes the parsed value with
  `serde_json::to_string` (`src/provider/anthropic.rs:258`).
- `tests/grouping_budget.rs:426` (`output_cap_rejection_falls_back_to_one_message_request`)
  asserts that Anthropic refuses grouping for the 515-file tree and that stderr contains
  "plan output cap". `tests/grouping_budget.rs:256` parses the inventory from the request.
- `cache::FINGERPRINT_VERSION_GOLDEN` (`src/cache.rs:378`) and its digest pin the fingerprint
  version; `plan_error_display_is_distinct` (`src/plan.rs:609`) enumerates the `PlanError`
  variants.
- `parse_defensive` (`src/plan.rs:80`) includes the CLO-517 recovery (`recover_groups`,
  `normalize_recovered_groups`) for near-miss shapes such as a `commits` key. Its tests
  (`src/plan.rs:676-860`) use path strings.
- Any `parse_defensive` error becomes `ErrorKind::Deserialize` (raw code `Deserialize`). A
  `plan::validate` error becomes a fallback whose raw code comes from
  `output::fallback_raw_code` (`src/output.rs:304`): `Deserialize` when the text contains
  "plan parse error", otherwise `PlanValidation`. The JSON raw codes are a frozen contract.
- `cache::FINGERPRINT_VERSION` (`src/cache.rs:28`, currently 3) must be bumped when the grouping
  prompt or schema changes, so a plan cached under the old contract re-analyzes.
- ADR-002 (`docs/adrs/002-library-boundary.md:238-252`) lists the `Provider` trait, `plan`,
  `diff`, `cache` and `budget` as binary-only. Changing `generate_plan`'s return type does not
  touch the library surface that `scripts/check-public-surface.sh` locks.

## 2. Acceptance Criteria

- [ ] AC1: `files_json` emits `{"i": <0-based position>, "path", "xy", "old_path"?}` for each
  entry, and `i` equals the entry's position in `changed`.
- [ ] AC2: `plan::schema()` and `plan::gemini_schema()` declare `groups[].files` as an array of
  integers (`"integer"` / `"INTEGER"`); every other property, `required` list and
  `additionalProperties`/`propertyOrdering` setting is unchanged.
- [ ] AC3: `GROUPING_SYSTEM_PROMPT` shows the integer shape in its example, tells the model to
  list inventory `"i"` values, states that every `"i"` appears in exactly one group, and no
  longer contains the renamed-file `"path"` / `"old_path"` rule. `grouping_user_content` names
  `{i, path, xy, old_path?}` and says to group by `"i"`.
- [ ] AC4: The parser accepts `groups[].files` items that are JSON integers or JSON strings, in
  any mix. A JSON integer that fits `usize` is an index (`FileRef::Index(usize)`); any string,
  including a numeric string such as `"12"`, is a path. A negative number, a number above
  `usize::MAX`, a number written with a fraction or exponent (`1.5`, `1.0`), `true`, `null`, an
  object or an array fails the parse with `PlanError::Parse`. The CLO-517 recovery (`groups`
  before `commits`, wrapper keys, DFS, near-miss group keys, and the `message` alias applied to
  the first group only) works unchanged for both item kinds.
- [ ] AC5: `main::build_plan` calls `plan::resolve` against the paths of `changed`, then
  `plan::validate`. `resolve` looks each index up with a checked `get`; an index
  `>= changed.len()` yields a new `PlanError::UnknownIndex(usize)`. A duplicated index becomes a
  duplicated path and `validate` rejects it with the existing `PlanError::DuplicateFile`. No
  input panics or wraps. Because `resolve` runs first, a plan that has both an out-of-range index
  and another validation error reports `UnknownIndex`; both outcomes are `PlanValidation`.
- [ ] AC6: The error classes are: a malformed reply (any `parse_defensive` failure) is raw code
  `Deserialize`; a resolution or partition failure (`UnknownIndex`, `UnknownFile`,
  `DuplicateFile`, `OmittedFile`, structure errors) is raw code `PlanValidation`. The
  `UnknownIndex` text does not contain "plan parse error". No new raw code is added.
- [ ] AC7: `Plan`, `Group`, `validate`, `validate_cached`, `validate_partition` and the cache file
  format (`CACHE_FORMAT_VERSION`) are unchanged, and the `validate*` tests and the
  `CACHE_FORMAT_VERSION` assertions pass unmodified. These expectations change on purpose:
  `FINGERPRINT_VERSION` and `FINGERPRINT_VERSION_GOLDEN` become 4 with the golden digest
  updated; `plan_error_display_is_distinct` grows from 7 to 8 variants; the existing path-string
  `parse_defensive` tests call `resolve` (or compare `FileRef::Path` items) before they compare
  paths.
- [ ] AC8: `PromptBudget::grouping` takes the file count and reserves the index echo, computed
  as `estimate_tokens` of the indices `0..n` joined by `,`, instead of
  `estimate_tokens(files_json) * PATH_ECHO_FACTOR`. `PATH_ECHO_FACTOR` is removed. The
  `OutputCap` rejection remains; its user message speaks of file indices and still contains the
  phrase "plan output cap". The check is an echo-only heuristic: it does not count the
  summaries and commit message, so a reply near the cap can still stop on `max_tokens`, and that
  stop falls back to a single commit as it does today. For Anthropic (`PLAN_MAX_TOKENS` 4096) a
  515-file change set (about 975 estimated index tokens) is no longer rejected; a 2,000-file
  change set (about 4,445) still is.
- [ ] AC9: Both Anthropic direct-deserialize shortcuts work on the wire type. The wire types
  derive `Serialize` so `src/provider/anthropic.rs:258` still compiles, or that shortcut
  serializes the raw `input` value instead. An Anthropic `tool_use` input with integer `files`
  yields a plan.
- [ ] AC10 (live): On the investigations repository rebuilt at `f2e2ad0` (2 modified, 513
  untracked; "Reproduction" section of the investigation doc), `gcm --plan-only --json` with the
  Vertex provider and `gemini-3.1-flash-lite` (or the current default flash-lite model if that
  one is unavailable; record which) returns an envelope with `mode: "plan_only"`,
  `cached: false`, no `fallback`, and groups whose files are exactly the 515 expected paths,
  each once. It runs with the default 60s timeout, and the total CLI wall time is 30s or less.
  The run pins `GCM_CONTEXT_WINDOW_TOKENS=1048576`, because `gemini-3.1-flash-lite` is not in the
  window table (`src/budget.rs:92`) and the 32,768-token `DEFAULT_WINDOW` would refuse the
  515-file grouping prompt before any request is sent. The same command on a `main` build, with
  the same provider, the same model, the same `GCM_CONTEXT_WINDOW_TOKENS` value and
  `GCM_HTTP_TIMEOUT_SECS=300`, gives the before time. Both times, the model and the window value
  go into the investigation doc.
- [ ] AC11 (live): The same tree with the Anthropic provider and the configured default Anthropic
  model meets the same envelope conditions, and the captured or logged plan request carries
  `max_tokens: 4096`. There is no `stop_reason: max_tokens` truncation and no
  `GroupingOutputCap` refusal. The same command on a `main` build is the baseline: it is
  expected to send no grouping request and to print the `GroupingOutputCap` refusal ("plan output
  cap") on stderr. The investigation doc records that refusal as the before result. If `main`
  instead sends a grouping request, record what happened and flag it in the PR, because the
  spec's premise for AC11 then no longer holds.
- [ ] AC12: A prompt-only reply (schema ignored, CLO-517 style, inside a ```json fence) parses
  and validates with integer indices and with path strings.

**Verification method**: AC1-AC9 and AC12 by `cargo test` unit tests in `src/plan.rs`,
`src/diff.rs`, `src/budget.rs`, `src/cache.rs`, `src/main.rs` and the provider modules, the
integration tests in `tests/grouping_budget.rs`, plus `cargo clippy --all-targets -- -D
warnings` and `scripts/check-public-surface.sh`. AC10 and AC11 by live runs timed with `time`
around the whole command (JSON mode turns off the progress ticker, so it does not report a
duration). If the implementer has no Vertex ADC or Anthropic credentials, AC10 and AC11 become
owner-run checks: record the exact commands in the PR and leave the criteria open.

## 3. Constraints

**Must**:
- Keep `Group.files: Vec<String>` and every validator signature. The index-to-path mapping is a
  separate step that produces a `Plan`.
- Map indices against the real `changed` slice in `main::build_plan`, not against the
  (possibly redacted) `ctx.files` text.
- Add a reply wire type in `src/plan.rs` (for example `PlanReply` / `ReplyGroup` with
  `files: Vec<FileRef>`, where `FileRef` is an untagged enum of `Index(usize)` and
  `Path(String)`), returned by `parse_defensive`, plus
  `plan::resolve(reply, &paths) -> Result<Plan, PlanError>`.
- Change `Provider::generate_plan` to return the wire type, and update the trait doc comment.
  This is an internal contract change; ADR-002 keeps the trait binary-only, so the library
  surface does not change.
- Change the five surfaces in Section 1 in one commit series so the schemas, the prompt and the
  budget never disagree on a released build.
- Keep the frozen JSON raw codes, with the error classes in AC6.
- Bump `FINGERPRINT_VERSION` from 3 to 4 with a doc line naming CLO-801.
- Keep `budget::GroupingRejected::OutputCap` and `PromptCause::GroupingOutputCap` as the
  refusal path for change sets whose index echo still exceeds the cap.
- Update the doc comments that still describe the path echo or the `{path, xy, old_path?}`
  inventory: `src/provider/anthropic.rs:140-141`, `src/budget.rs:26`, `:337`, `:346`, `:423`,
  and `src/diff.rs:20`, `:142`.

**Must-not**:
- Do not send the indices as a separate payload; the `"i"` field in the inventory is the only
  index source.
- Do not advertise strings in the wire schemas or the prompt. The string tolerance is a parser
  behavior only.
- Do not drop or clamp an out-of-range index.
- Do not raise `PLAN_MAX_TOKENS` or `DEFAULT_TIMEOUT_SECS`.
- Do not change the message-call path, `SYSTEM_PROMPT`, or the message budget.
- Do not change `output::fallback_raw_code` in this task. It can mislabel a `DuplicateFile`
  whose path contains "plan parse error"; that bug predates CLO-801 and goes to the PR body as a
  follow-up.

**Prefer**:
- Every provider keeps its current one-line `parse_defensive` call.
- Reuse `estimate_tokens` for the index echo rather than a new per-file constant.
- Keep the existing path-string tests in `src/plan.rs` close to their current form; they now
  also cover the string fallback. Add index-based tests beside them.

**Escalate when**:
- Groq strict mode, Gemini/Vertex `responseSchema`, OpenAI structured outputs or Anthropic
  tool use rejects an integer array item in a live call.
- A live model returns indices that validate but group files visibly wrong (for example,
  consistently off by one), because that means the `"i"` field does not prevent miscounting.
- AC10 shows the run still above 30s, which means output tokens were not the only cost.
- Any change to the library surface check output is needed.

## 4. Decomposition

1. **Inventory index**: add `i` to `InventoryEntry` in `files_json`; update the tests that parse
   the inventory and the doc comments. - files: `src/diff.rs`
2. **Wire type, parser and resolver**: add the reply wire type with the integer-or-string item,
   make `parse_defensive` (including the CLO-517 recovery) return it, add `plan::resolve` and
   `PlanError::UnknownIndex`, and change both schemas to integer items. Add the tests in rows
   2-12 and 21 of Section 5. - files: `src/plan.rs`
3. **Prompt and providers**: rewrite `GROUPING_SYSTEM_PROMPT` and the `grouping_user_content`
   header; change `Provider::generate_plan` to return the wire type in the trait and all six
   providers; move Anthropic's two direct-deserialize shortcuts to the wire type; update the
   payload and schema tests. - files: `src/provider/facade.rs`, `src/provider/anthropic.rs`,
   `src/provider/gemini.rs`, `src/provider/vertex.rs`, `src/provider/groq.rs`,
   `src/provider/openai.rs`, `src/provider/ollama.rs`
4. **Build plan, budget, cache and integration tests**: call `plan::resolve` with the paths
   from `changed` before `plan::validate` in `build_plan`; before the resolve call, log one
   `debug`-level line with the number of integer and string file references in the reply (for
   example `plan reply: 512 index refs, 3 path refs`), so a live run with `GCM_LOG_LEVEL=debug`
   shows the reply shape without a provider change; replace the path-echo reserve with
   the index-echo reserve, pass the file count instead of `files_json`, reword the
   `GroupingOutputCap` message and the module doc; bump `FINGERPRINT_VERSION` and its golden;
   update the budget unit tests. In `tests/grouping_budget.rs`, change the 515-file Anthropic
   case so it expects one `tool_use` grouping request with `max_tokens: 4096`, answered with an
   integer-index plan, and a grouped result; add a 2,000-file Anthropic case that expects no
   grouping request and stderr containing "plan output cap"; update the inventory parse at line
   256 for the `i` field. Change `grouping_cannot_fit_falls_back_to_a_sent_message`
   (`tests/grouping_budget.rs:564`): lower its `GCM_CONTEXT_WINDOW_TOKENS` from 80,000 to 35,000,
   because the smaller index-echo reserve lets the 515-file grouping prompt fit in 80,000, and
   keep the assertion that only the fallback message request is sent (`requests.len() == 1`, no
   `response_format`). - files: `src/main.rs`, `src/budget.rs`, `src/cache.rs`,
   `tests/grouping_budget.rs`
5. **Live re-measure (owner-run when credentials are missing)**: prerequisites are Vertex ADC,
   an Anthropic key, an Ollama cloud key and the investigations repository with commit
   `f2e2ad0`. Rebuild the 515-file tree, run AC10 and AC11 on this branch and on a `main` build,
   run one Ollama cloud passthrough model on the same tree, uncached, with
   `GCM_CONTEXT_WINDOW_TOKENS` pinned to that model's documented context window (record the
   value and its source), and append the commands, models, window values and wall times to the
   CLO-797 investigation doc. For the Ollama run, record the number of integer and string file
   references in the reply before resolution, from the `debug` line of sub-task 4
   (`GCM_LOG_LEVEL=debug`). A valid partition with integer references
   only counts as prompt adherence. A successful plan from a reply with path strings only, or
   with integers and strings mixed, is recorded as a compatibility outcome, not as adherence. - files:
   `docs/investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md`

**Dependency order**: 1 and 2 are independent. 3 depends on 2. 4 depends on 1, 2 and 3 (the
crate compiles only after 2-4 land together). 5 depends on 1-4.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `files_json` on three changed files, one renamed | each entry has `i` 0, 1, 2 in order; the renamed one keeps `old_path` | `cargo test diff::` |
| 2 | `schema()` and `gemini_schema()` item types | `files.items.type` is `"integer"` / `"INTEGER"`; other keys unchanged | `cargo test plan::` |
| 3 | `parse_defensive` on `{"groups":[{"files":[0,2],...},{"files":[1],...}]}` then `resolve` against `[a,b,c]` | groups `[a,c]` and `[b]` | `cargo test plan::` |
| 4 | Inventory not in alphabetical order (`[z.rs, a.rs, m.rs]`), reply `[[2,0],[1]]` | groups `[m.rs, z.rs]` and `[a.rs]`, in reply order | `cargo test plan::` |
| 5 | Mixed items `[0, "b.rs"]` | resolves to `[a.rs, b.rs]` | `cargo test plan::` |
| 6 | Numeric string `["1"]` with a file literally named `1` in the change set; and `["0"]` with no file named `0` | the first validates as the path `1`; the second fails with `UnknownFile("0")` (fails closed) | `cargo test plan::` |
| 7 | Index `3` against three files | `PlanError::UnknownIndex(3)`; `fallback_raw_code` gives `PlanValidation` | `cargo test plan::` and `cargo test output::` |
| 8 | Items `-1`, `1.5`, `1.0`, `18446744073709551616`, `true`, `null`, `{}` | each fails with `PlanError::Parse`, no panic | `cargo test plan::` |
| 9 | Index `0` in two groups | `validate` returns `DuplicateFile(<path of 0>)` | `cargo test plan::` |
| 10 | `commits` alias with integer `files` and a `message` on groups 0 and 1 (CLO-517 shape) | recovered, resolves, validates; only group 0 gets `commit_message` | `cargo test plan::` |
| 11 | Existing path-string `parse_defensive` and `validate*` tests | pass; only the resolve step is added before the path comparison | `cargo test plan::` |
| 12 | 515-entry integer reply | parses, resolves and validates | `cargo test plan::` |
| 13 | `PromptBudget::grouping` for Anthropic with 515 files | `Ok`; reserve equals `estimate_tokens` of `"0,1,...,514"` plus the fixed parts | `cargo test budget::` |
| 14 | `PromptBudget::grouping` for Anthropic with 2,000 files | `Err(OutputCap { .. })`; message says "indices" and contains "plan output cap" | `cargo test budget::` |
| 15 | Anthropic `tool_use` input with integer `files` | the direct path returns the wire plan | `cargo test provider::anthropic` on `extract_tool_use_input` plus the wire-type parse |
| 16 | Anthropic `stop_reason: max_tokens` on the plan call | `Deserialize` error, and `build_plan` falls back to a single commit | `cargo test provider::anthropic` (existing truncation test) plus a `tests/grouping_budget.rs` capture case |
| 17 | Every provider's plan payload | carries the new prompt and the integer schema | `cargo test provider::` |
| 18 | `build_plan` resolve-then-validate with a stub reply | paths come from `changed`; out-of-range gives a `PlanValidation` fallback | `cargo test` in `src/main.rs`, through a helper that runs resolve and validate, or `Repo::at` on a temporary repo with a stub `Provider` |
| 18a | Reply-shape `debug` line in `build_plan` with a mixed reply `[0, "b.rs", 2]` | stderr at `GCM_LOG_LEVEL=debug` shows 2 index refs and 1 path ref; nothing at the default level | `cargo test` in `src/main.rs` (same helper as row 18), or a `tests/grouping_budget.rs` capture case |
| 19 | Redacted path: the inventory text sent to the model has a redacted path, the reply uses its index | the plan holds the real path from `changed` | `cargo test` in `src/main.rs` (same helper as row 18) |
| 20 | Renamed file: the reply uses the index of a rename | the plan holds the new path; the old path appears nowhere in the plan | `cargo test` in `src/main.rs` (same helper as row 18) |
| 21 | Prompt-only reply in a ```json fence with integer indices, and one with paths | both parse, resolve and validate | `cargo test plan::` |
| 22 | `FINGERPRINT_VERSION` | 4; the golden test is updated; a plan cached under version 3 is a miss | `cargo test cache::` |
| 23 | 515-file Anthropic run through the binary (`tests/grouping_budget.rs`) | one `tool_use` request with `max_tokens: 4096` whose inventory entries carry `i`; the integer-index reply gives a grouped result | `cargo test --test grouping_budget` |
| 24 | 2,000-file Anthropic run through the binary | no grouping request; stderr contains "plan output cap" | `cargo test --test grouping_budget` |
| 24a | `grouping_cannot_fit_falls_back_to_a_sent_message` with `GCM_CONTEXT_WINDOW_TOKENS=35000` | exactly one request, the fallback message without `response_format`; envelope `mode: "single"` | `cargo test --test grouping_budget` |
| 25 | Full suite and lints | all pass | `cargo test && cargo clippy --all-targets -- -D warnings && scripts/check-public-surface.sh` |
| 26 | Live Vertex grouping on the 515-file tree (AC10) | envelope conditions of AC10; wall time 30s or less; before time recorded with the same model and window | rebuild per the investigation doc "Reproduction" section, then `GCM_CONTEXT_WINDOW_TOKENS=1048576 time gcm --plan-only --json` with the Vertex provider, on this branch and on `main` (`main` also with `GCM_HTTP_TIMEOUT_SECS=300`) |
| 27 | Live Anthropic grouping on the 515-file tree (AC11) | branch: envelope conditions of AC11, `max_tokens: 4096`, no truncation; `main`: no grouping request and the "plan output cap" refusal, recorded as the baseline | same tree, Anthropic provider, on this branch and on `main` |
| 28 | Live Ollama cloud passthrough model on the 515-file tree, uncached (owner-run) | integer and string reference counts from the raw reply, before resolution; outcome classed as prompt adherence (valid all-integer partition), compatibility (valid path-only or mixed reply) or failure (with the reply shape) | same tree, Ollama provider with a cloud model, `GCM_CONTEXT_WINDOW_TOKENS` pinned to the model's documented window |

**Edge cases to verify**:
- A model that numbers from 1: index `n` is out of range or index `0` is omitted, so validation
  rejects the plan instead of committing a wrong grouping.
- A model that ignores the prompt and emits paths only: the plan validates through the string
  fallback.
- An empty `files` array in a group: still `PlanError::EmptyGroup` after resolution.
- A cached plan from before this change: re-analyzed because of the fingerprint bump, not
  mis-read.
