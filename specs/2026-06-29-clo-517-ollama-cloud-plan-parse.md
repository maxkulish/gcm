# Spec: Fix Ollama cloud model commit-plan parse failure (CLO-517)

**Created**: 2026-06-29
**Linear**: [CLO-517](https://linear.app/cloud-ai/issue/CLO-517)
**Estimated scope**: S (3 source files: `src/provider/mod.rs`, `src/cache.rs`, `src/plan.rs` + tests; ~3 sub-tasks)
**Reference**: `docs/hotfix/2026-06-29-ollama-cloud-plan-parse-failure.md`
**Status**: APPROVED (2026-06-29) - dual-model AI review (Gemini + Ollama: APPROVE_WITH_SUGGESTIONS) + owner checkpoint review, all points validated against source.

## Review Validation (owner checkpoint - CONFIRMED)

Four blind spots raised at the spec checkpoint, each validated against the code and folded in:

1. **Missing `summary` paradox - CONFIRMED CRITICAL.** `Group.summary: String` (`plan.rs:22`)
   has no `#[serde(default)]`, so `from_value::<Plan>` errors on a missing `summary`. The exact
   bug payload (`{"commits":[{"message":...,"files":[...]}]}`, hotfix lines 33/53) has **no**
   `summary`, so the recovery must **synthesize** it. `summary` is display-only (`main.rs:804-806`),
   never committed nor content-validated, so synthesis is safe. -> AC + normalizer updated.
2. **DFS alias inconsistency - CONFIRMED.** `find_groups_dfs` (`plan.rs:212`) finds `groups`
   anywhere; the `commits` alias must be searched the same way. -> AC added.
3. **All groups receiving `commit_message` - CONFIRMED non-issue.** `validate` checks only
   `groups[0]` (`plan.rs:302`, doc-comment `:287`); `validate_cached` skips it; `main.rs:470`
   reads only `groups[0]`. Uniform "map when absent" is harmless. -> documented decision.
4. **`description`/`title` -> `summary` alias - CONFIRMED secondary.** The real payload had no
   summary alias at all (only synthesis rescues it), but the alias is cheap bulletproofing. ->
   folded into the normalizer before synthesis.

## 1. Problem Statement

Multi-commit grouping silently fails for Ollama **cloud passthrough** models (the
`:cloud` / `-cloud` tag, e.g. `nemotron-3-nano:30b-cloud`). `gcm` prints to stderr:

```
gcm: could not parse the Ollama response: plan parse error: could not extract a commit plan from the response. Falling back to single-commit mode.
```

and degrades to single-commit mode. The single-commit (message) path works; only the
grouping path fails.

**Root cause (verified, see hotfix doc).** The grouping request asks for structured
output by sending Ollama's `format` JSON schema (`build_plan_payload` in
`src/provider/ollama.rs:188`, schema from `src/plan.rs:229` / `schema()`). Ollama enforces
that schema through grammar-constrained decoding **only for local GGUF models**. For cloud
passthrough models the `format` field is a no-op - the remote model never receives the
schema. The shared grouping prompt `GROUPING_SYSTEM_PROMPT` (`src/provider/mod.rs:341`)
names the fields (`groups[0]`, `summary`, `commit_message`) but never states the actual JSON
shape or gives an example; the full schema lived only in the `format` field. So the cloud
model guesses the wrapper and returns a near-miss shape:

```json
{ "commits": [ { "message": "...", "files": ["..."] } ] }
```

instead of the required shape:

```json
{ "groups": [ { "files": ["..."], "summary": "...", "commit_message": "..." } ] }
```

`parse_defensive` / `recover_groups` (`src/plan.rs:80`, `:189`) only recognize a top-level
`groups` key (plus known wrapper keys and a DFS for a nested `groups` array), so no candidate
yields a `Plan`. This produces `PlanError::Parse("could not extract a commit plan from the
response")`, and `main.rs:770` announces the single-commit fallback.

**Who is affected**: anyone using an Ollama cloud passthrough model (or any provider/model
that ignores or weakly honors structured-output `format`/`response_format`) for grouping.

## 2. Acceptance Criteria

- [ ] `GROUPING_SYSTEM_PROMPT` (`src/provider/mod.rs:341`) fully specifies the output JSON
      shape in prose: the top-level key MUST be `groups` (array, never `commits`); each group
      has `files` (array of exact paths), `summary` (string), and `commit_message` (full
      conventional commit on `groups[0]` only, `null` for every other group); and it includes
      a short literal example object.
- [ ] The doc-comment above `GROUPING_SYSTEM_PROMPT` (currently claims "the structured-output
      schema enforces the shape, so the prompt carries only the grouping rules") is updated -
      that statement becomes false once the prompt restates the shape. It must note the prompt
      now carries the shape for providers that don't enforce `format`/`response_format`.
- [ ] **`FINGERPRINT_VERSION` is bumped `2` -> `3` in `src/cache.rs:26`.** The doc-comment at
      `cache.rs:22-24` mandates a bump whenever the grouping prompt or schema changes;
      changing `GROUPING_SYSTEM_PROMPT` without it would reuse stale cached plans built under
      the old prompt contract. A unit test asserts the fingerprint changes across the bump (or
      asserts the version constant is `3`).
- [ ] `recover_groups` (`src/plan.rs:189`) recovers the near-miss shape. The **exact**
      payload from the bug report - `{"commits":[{"message":"...","files":["..."]}]}` with **no
      `summary` key** (hotfix doc lines 33, 53) - parses to a valid `Plan` without a
      `PlanError::Parse`. This requires, per recovered group:
      - a top-level `commits` array treated as `groups` (also recognized under existing known
        wrapper keys, e.g. `{"result":{"commits":[...]}}`, and via the DFS - see next AC);
      - `message` mapped to `commit_message` (only when `commit_message` is absent);
      - `summary` **synthesized** when absent: from the group's resolved `commit_message`/`message`
        first line if present, else a deterministic fallback (e.g. file count / first path).
        `summary` is display-only (`main.rs:804-806`); it is never committed nor content-validated,
        so synthesis is safe. Optionally accept `description`/`title` as `summary` aliases first.
- [ ] DFS consistency: `find_groups_dfs` (`src/plan.rs:212`) recovers a deeply-nested `commits`
      array the same way it recovers a nested `groups` array (search for `groups` first, then
      `commits`), so the `commits` alias is not silently dropped when nested below the known
      wrapper keys.
- [ ] Precedence preserved: when a response contains BOTH `groups` and `commits`, the strict
      `groups` path wins (direct `Plan` parse / `groups` recovery is tried before the `commits`
      alias, at every level: top-level, known wrappers, and DFS).
- [ ] Existing local-model behavior is unchanged: the `format` field is still sent by
      `build_plan_payload`; `schema()` is unchanged; the strict `groups`/`commit_message`
      shape remains the primary contract (direct `Plan` deserialize is still tried first).
- [ ] A unit test covers the `commits`/`message` near-miss shape end-to-end through
      `parse_defensive`.
- [ ] All existing `plan.rs` parse tests still pass; `cargo fmt` and `cargo clippy` are clean.
- [ ] (Optional/docs) A one-line note in the `ollama` module doc (`src/provider/ollama.rs`)
      that Ollama cloud (`:cloud`/`-cloud`) models do not enforce `format`, so structured
      output relies on the prompt-level schema.

**Verification method**: `cargo test plan::` (parser unit tests), `cargo test` (full suite),
`cargo fmt --check`, `cargo clippy -- -D warnings`. Manual: run `gcm` against
`nemotron-3-nano:30b-cloud` and confirm a multi-commit plan is produced (no fallback).

## 3. Constraints

**Must**:
- Keep the `format` field in `build_plan_payload` (`src/provider/ollama.rs`) and `schema()`
  in `src/plan.rs` unchanged - local GGUF grammar-constrained decoding must keep working.
- Keep the strict shape as the primary contract: `parse_defensive` still tries a direct
  `Plan` deserialize first; recovery is a fallback only.
- Preserve `groups[0]`-only commit-message semantics and all `validate` rules - recovery only
  normalizes key names, it does not relax validation (a recovered plan with a null/missing
  `groups[0]` message must still raise `MissingFirstMessage`).

**Must-not**:
- Do not change `schema()` or the `response_format`/`format` payload shape.
- Do not make `commits`/`message` the canonical shape - the prompt must still instruct models
  to emit `groups`/`commit_message`.
- Do not break any existing `parse_defensive` test (wrapper key, nested DFS, bare array,
  brace-in-string, decoy brace, garbage-is-error).

**Prefer**:
- Implement recovery as a general `normalize_recovered_groups(Value) -> Value` helper applied
  to the recovered `groups` array before `from_value`, so every recovery path (bare array,
  known wrappers, DFS, the new `commits` alias) normalizes per-group keys uniformly. Per group:
  - map `message` -> `commit_message` only when `commit_message` is absent (a real
    `commit_message` always wins; no double-map). Apply uniformly to all groups - later groups
    carrying a message is harmless: `validate` only checks `groups[0]` and later messages are
    "tolerated/ignored" (`plan.rs:287`), `validate_cached` skips the check, and `main.rs:470`
    reads only `groups[0]`.
  - synthesize `summary` only when absent (after trying `description`/`title` aliases): first
    non-empty line of the resolved `commit_message`, else a deterministic fallback. A real
    `summary` always wins.
  - do NOT synthesize `files`: an absent/empty `files` must still fail (it is the partition
    key; there is nothing safe to invent).
- Add a `tracing::debug!` when recovery uses the `commits`/`message`/synthesized-`summary`
  path, for observability of future model drift.
- Keep the prompt addition concise; restate only the shape + one example, not the full schema.

**Escalate when**:
- The recovery change would require relaxing `validate` semantics (it should not).
- Real-model testing shows the prompt change alone is insufficient AND the recovery does not
  catch the observed shape (would indicate a third shape variant worth a separate ticket).

## 4. Decomposition

1. **Prompt + cache fingerprint** - Extend `GROUPING_SYSTEM_PROMPT` to state the top-level
   `groups` key, per-group fields, the `groups[0]`-only message rule, and a literal example
   object. Update the doc-comment above it (currently claims "the structured-output schema
   enforces the shape, so the prompt carries only the grouping rules") to reflect that the
   prompt now restates the shape for providers that don't enforce `format`. **Bump
   `FINGERPRINT_VERSION` `2` -> `3` in `src/cache.rs:26`** (required by the `cache.rs:22-24`
   convention since the grouping prompt changed) and add/adjust a test asserting the constant
   is `3` (or that the fingerprint differs from a version-2 baseline). - files:
   `src/provider/mod.rs`, `src/cache.rs`

2. **Parser: tolerate `commits`/`message` aliases + synthesize `summary`** - Add a
   `normalize_recovered_groups(Value) -> Value` helper and apply it to the recovered `groups`
   array in `parse_defensive` before `from_value`. Per group: map `message` -> `commit_message`
   when `commit_message` absent; resolve `summary` from `description`/`title` aliases else
   synthesize it (first line of the resolved message, else a deterministic fallback) when
   absent; never synthesize `files`. Extend `recover_groups` to treat a top-level `commits`
   array (and a `commits` array under the existing known wrapper keys) as the `groups` array,
   and extend `find_groups_dfs` to find a nested `commits` array (after `groups`). Keep
   precedence at every level: `groups` before `commits`. Add a `tracing::debug!` on
   alias/synthesis use. - files: `src/plan.rs`

3. **Tests + docs** - Add `parse_defensive` unit tests: (a) the exact bug payload
   `{"commits":[{"message":"feat: a","files":["a"]}]}` (no `summary`) -> valid `Plan`, correct
   files, `groups[0].commit_message == Some("feat: a")`, non-empty synthesized `summary`; (b) a
   `commits` alias with a second group `message: null` -> `groups[1].commit_message == None`;
   (c) both `groups` and `commits` present -> `groups` wins; (d) a `commits` group missing
   `files` (or empty `files`) still fails (recovery does not invent the partition key); (e)
   nested `{"result":{"commits":[...]}}` recovers; (f) a real `summary` is preserved (synthesis
   does not override it). Add the one-line cloud-`format` note to the `ollama` module doc. -
   files: `src/plan.rs` (tests), `src/provider/ollama.rs` (doc note).

**Dependency order**: Sub-tasks 1 and 2 are independent and can be done in either order.
Sub-task 3's tests depend on sub-task 2. The primary fix (sub-task 1, prompt) alone satisfies
the verified reproduction; sub-task 2 is defense-in-depth that the unit-test acceptance
criteria depend on. The `FINGERPRINT_VERSION` bump (part of sub-task 1) is mandatory whenever
the prompt changes, independent of sub-task 2.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | New unit test: exact bug payload `{"commits":[{"message":"feat: a","files":["a"]}]}` (no `summary`) | `Plan` with `groups[0].files == ["a"]`, `commit_message == Some("feat: a")`, non-empty synthesized `summary` | `cargo test plan::` |
| 2 | New unit test: `commits` alias with a second group carrying `message: null` | `Plan` with 2 groups; `groups[1].commit_message == None` | `cargo test plan::` |
| 3 | Regression: all existing `parse_defensive_*` tests | Pass unchanged | `cargo test plan::` |
| 4 | Regression: strict shape still wins | `{"groups":[...]}` still parses via the direct `Plan` path | `cargo test plan::` |
| 5 | New unit test: both `groups` and `commits` present | `groups` path wins | `cargo test plan::` |
| 6 | New unit test: `commits` group missing/empty `files` | Still fails (recovery doesn't invent the partition key) | `cargo test plan::` |
| 6b | New unit test: real `summary` present on a `commits` group | Preserved (synthesis does not override) | `cargo test plan::` |
| 6c | New unit test: nested `{"result":{"commits":[...]}}` | Recovers to a valid `Plan` | `cargo test plan::` |
| 7 | New/adjusted test: `FINGERPRINT_VERSION` | Constant is `3` (or fingerprint differs from v2 baseline) | `cargo test cache::` |
| 8 | Prompt contains shape + example | `GROUPING_SYSTEM_PROMPT` mentions `groups`, forbids `commits`, includes an example object | grep / code review |
| 9 | Local-model payload unchanged | `build_plan_payload` still sends `format`; `schema()` unchanged | `cargo test`, diff review |
| 10 | Full suite + lint | All tests pass; fmt + clippy clean | `cargo test && cargo fmt --check && cargo clippy -- -D warnings` |
| 11 | Manual real-model | `gcm` against `nemotron-3-nano:30b-cloud` produces a multi-commit plan, no fallback message on stderr | manual run |

**Edge cases to verify**:
- A group with `message` present AND `commit_message` present (real `commit_message` wins; no double-map).
- A group with `summary` present AND a `message`/`description` (real `summary` wins; not overwritten by synthesis).
- The exact bug payload (no `summary` at all) recovers via synthesis - this is the primary
  defense-in-depth target, NOT a failure case.
- `commits` array where `groups[0]` has `message: null` (and no `commit_message`) -> still
  raises `MissingFirstMessage` via `validate` (synthesis fills `summary`, never the message).
- A `commits` group with absent/empty `files` -> still fails (`files` is never synthesized).
- A response with both a `commits` array and a `groups` array (strict `groups`
  precedence preserved - direct/`groups` path tried before the `commits` alias, at every level).
- A `commits` array nested under a known wrapper key (`{"result":{"commits":[...]}}`) and a
  deeply-nested `commits` (DFS) both recover.
- Bare top-level array and existing wrapper-key shapes continue to recover (no regression).
