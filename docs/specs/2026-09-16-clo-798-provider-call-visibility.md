# Spec: Progress and failure visibility for provider calls

**Linear**: [CLO-798](https://linear.app/cloud-ai/issue/CLO-798)
**Created**: 2026-09-16
**Revised**: 2026-09-16 (v2)
**Estimated scope**: M/L (8 files, 7 sub-tasks)
**Related**: [CLO-797](https://linear.app/cloud-ai/issue/CLO-797) (oversized grouping prompt - the input defect behind both symptoms). CLO-798 makes the failure legible; CLO-797 removes the cause. They are independent.

> **v2** rewrites sections 2-5 after a code-grounded review reproduced seven defects in v1. The v1 plan was unachievable in three places: promoting retries to `Warn` emitted nothing (the logger defaults to `Off`), a `pub(crate)` helper in the library could not be called from the binary (separate crates), and the timeout message keyed on a budget the error does not carry. The table "Findings that reshaped this spec" records each with its evidence.

## 1. Problem Statement

`gcm` prints nothing between "start" and "done" while an LLM provider call is in flight. A run against `gemini-3.5-flash-lite` sat silent for minutes with no indication of what it was doing, whether it was retrying, or that it was about to time out. A Groq run in the same session failed with a raw provider HTTP 400 that told the user nothing actionable. There is no way to tell a slow model from a hung process.

The silence is by construction, not by accident:

* **Every log call in the codebase is `debug_log!`** (`src/debug.rs:74`), i.e. `Level::Debug`, and the default level is `Level::Off` (`src/debug.rs:39-48`). There are 8 call sites total: `main.rs:415,462,495`, `plan.rs:279,294`, `provider/http.rs:142,207,329`. With default settings the generation path emits zero bytes on stderr.
* **No progress indicator on the generation call.** The only spinner in the codebase is `cliclack::spinner()` in the `gcm provider` wizard (`src/config/facade.rs:162,178`), which never runs on the commit path.
* **The transport is synchronous.** `post_json` (`src/provider/http.rs:67`) blocks the calling thread on `ureq` per ADR-001 Decision 2. Nothing can print during the call without a second thread.
* **Timeout is 60s** (`DEFAULT_TIMEOUT_SECS`, `src/provider/http.rs:20`), overridable via `GCM_HTTP_TIMEOUT_SECS` (`src/provider/http.rs:40-44`). The user is told neither number when it fires: `ErrorKind::Timeout` renders as the bare `"{p} API request timed out"` (`src/provider/identity.rs:56`).
* **`Timeout` is not retryable** (`is_retryable`, `src/provider/identity.rs:97-99` retries only `RateLimit` and `Server`), so a single stalled request fails after 60s.
* **Retries can still reach minutes.** `DEFAULT_MAX_RETRIES = 3` (4 attempts) with backoff base 500ms / max 8s (`src/provider/http.rs:28-32`). Four attempts each running to the 60s timeout is ~4 minutes of silence. Each retry *is* logged, but at Debug only (`src/provider/http.rs:329`).
* **A context-window rejection is indistinguishable from a gcm bug.** Providers reject an oversized prompt with HTTP 400; `classify_status` maps that to `ErrorKind::BadRequest` (`src/provider/http.rs:215`), whose `Display` says *"Likely an unsupported model/parameter or a gcm bug; please report it."* (`src/provider/identity.rs:43-50`). That text is wrong for the most common 400 gcm produces, and it asks the user to file a bug for their own oversized diff.
* **A second provider call can start with no explanation.** A grouping failure falls through to the single-message path (`src/main.rs:494`), but the announcement is gated on `!args.json` (`src/main.rs:903`). A `--json` consumer sees two requests and is told why for neither.

Who is affected: every interactive `gcm` user on a slow provider or a large diff, and every automated `--json` consumer whose run stalls with no log line to explain it.

**Not reproduced**: the exact cause of the multi-minute `gemini-3.5-flash-lite` stall is unknown - precisely because nothing was logged. This spec does not claim a cause; it makes the next occurrence self-diagnosing.

### There are four provider call sites, not one

Every one of them is currently silent, and each needs a different operation label and different failure advice:

| # | Call | Site | Operation label | On context-window failure |
|---|------|------|-----------------|---------------------------|
| 1 | `generate_plan` | `build_plan`, `src/main.rs:553` | `grouping` | suggest `--all` or a narrower scope |
| 2 | `generate_message` | `commit_first_group` (cached-group regeneration), `src/main.rs:620` | `group message` | suggest a narrower scope; **not** `--all` |
| 3 | `generate_message` | `single_commit_path`, `src/main.rs:782` | `single-commit message` | `--all` is already in effect - suggest committing in smaller batches or raising `GCM_DIFF_TOTAL_BYTES` |
| 4 | `generate_message` | via `run_fallback` -> `single_commit_path`, `src/main.rs:892-906` | `fallback message` | same as 3, and the transition itself must be announced |

### Decisions taken before drafting (v1) and after the review (v2)

1. **Hand-rolled ticker, not `cliclack::spinner`.** `cliclack` is a wizard widget: it owns its own stderr formatting and has no non-TTY fallback.
2. **`--json` suppresses the ticker only.** The status line, retry notices and transition announcements still go to stderr under `--json`.
3. **JSON contract: schema and codes freeze, prose may improve.** `error.message` (`src/output.rs:194`) and `fallback.reason` (`src/output.rs:180`) carry exactly the text this ticket exists to fix. Freezing their bytes would leave a `--json` consumer reading *"Likely ... a gcm bug; please report it"* for their own oversized diff. So the envelope shape, `v`, `status`, `mode`, `error.code` and `fallback.raw_code` are frozen; the two human-prose fields may change.
4. **The logger's default level moves `Off` -> `Warn`.** Without this, item 3 of the ticket's scope is a no-op. `GCM_LOG_LEVEL=off` is the opt-out.
5. **Timeout and context-window prose are composed binary-side**, where the operation and its actual budget are known. `ErrorKind` keeps its shape.

## 2. Acceptance Criteria

- [ ] **AC-1** A run against a deliberately slow endpoint shows visible stderr activity within 2 seconds of the provider call starting, and thereafter never goes more than 5 seconds without further output until the call resolves. **Exception**: under `--json` the ticker does not run, so only the status line, retry notices and transition announcements appear (AC-7).
- [ ] **AC-2** Before each of the four provider calls, a default-on stderr status line names the **operation** (`grouping`, `group message`, `single-commit message`, `fallback message`), the provider, the model, the changed-file count, and the approximate prompt size - the byte count of the generated prompt text sections per sub-task 5, not tokens and not the serialized request body.
- [ ] **AC-3** With `GCM_LOG_LEVEL` and `GCM_DEBUG` both unset, a retried request states on stderr that it is retrying, which attempt it is on out of how many, and how long it will wait. This requires the default level to become `Warn`; `GCM_LOG_LEVEL=off` silences it, `GCM_LOG_LEVEL=error` silences it, an unparseable value falls back to `Warn`, and `GCM_DEBUG=1` still selects `Debug`.
- [ ] **AC-4** A timed-out generation call produces a message naming the **budget that actually applied** and the `GCM_HTTP_TIMEOUT_SECS` override, for **both** timeout phases: no response headers (`ErrorKind::Timeout`) and a stall after 2xx headers while reading the body (`ErrorKind::Transport` whose text contains `timeout`). The `gcm provider` model-list fetch, which uses a fixed 5s budget (`src/provider/http.rs:37,98`), must **not** be described using the 60s generation budget.
- [ ] **AC-5** A context-window rejection produces a gcm-authored message naming the cause, the changed-file count, the approximate prompt size and **operation-specific** advice per the table in section 1. It never says "a gcm bug; please report it". Detection survives both extraction hazards: a signal carried only in a sibling `error.code`/`error.type`, and a signal appearing beyond character 200 of `error.message`.
- [ ] **AC-6** `GCM_LOG_LEVEL=debug` logs the byte size of each grouping-prompt section (`file_list`, `status`, `stat`, `body`) plus the total.
- [ ] **AC-7** `--json` stdout keeps a frozen contract: envelope shape, `v`, `status`, `mode`, `error.code` and `fallback.raw_code` are byte-identical to the pre-change output for every envelope status (`plan`, `noop`, `committed`, `fallback`, `error`). `error.message` and `fallback.reason` may change text. The ticker does not run under `--json`.
- [ ] **AC-8** When stderr is not a TTY, no ANSI escape and no `\r` is written; progress appears as whole plain lines.
- [ ] **AC-9** No breaking change to the public library surface: no existing item in `gcm::provider` is removed or changed in signature, `ErrorKind` keeps its exact variants and shapes, `tests/library_provider_api.rs` passes unchanged, and `scripts/check-public-surface.sh` passes. Additive pure helpers are permitted and must be listed in the PR description; the surface script only rejects eight named types (`scripts/check-public-surface.sh:22-31`) and is **not** evidence that nothing was added.
- [ ] **AC-10** A retry notice, a debug line or any other stderr write emitted while the ticker is live never lands on top of the progress line: the progress line is cleared first, the message is written whole, and the ticker resumes on the next line.
- [ ] **AC-11** The grouping-to-single-commit transition is announced on stderr **including** under `--json`, naming the reason. A run that makes two provider calls never presents two unexplained status lines.
- [ ] **AC-12** A provider call that fails immediately (e.g. `MissingKey`, which Groq raises **inside** `generate_plan` at `src/provider/groq.rs:62`) leaves no ticker fragment on stderr and adds no measurable delay: `finish()` on a non-TTY ticker must not wait out the 5s interval.

**Verification method**: the table in section 5. Every criterion has a numbered test.

## 3. Constraints

**Must**:
- All new human-readable output goes to **stderr**. `println!` is not added anywhere on the generation path; `src/output.rs:205` stays the only stdout writer.
- The status line, retry notices, transition announcements and the timeout/context-window messages are on by default, with `GCM_LOG_LEVEL` unset.
- **The progress/log coordination flag is a single static in the library.** `src/debug.rs` is compiled into **both** crates (`src/lib.rs:32` `pub mod debug` and `src/main.rs:5` `mod debug`), so a flag declared there exists twice and the binary's ticker would never be seen by the library's `log!`. The shared state lives in `gcm::debug` and the binary reaches it by that path, not through its own `crate::debug`.
- The ticker's wait is **interruptible** (a `Condvar` or `mpsc::recv_timeout`, never a bare `thread::sleep`), so `finish()` returns promptly on a fast call.
- The ticker thread is always joined before `gcm` prints its next line or exits, including on the error path. No detached thread outlives the call. (`src/main.rs:36-39` is `std::process::exit(run(&args))`; `run` returns normally, so a `CallProgress` local in `run` drops before `exit` - destructors are not skipped.)
- `ErrorKind` keeps its current variants and shapes. `gcm::provider::ErrorKind::Http(503)` (exercised by `tests/library_provider_api.rs:41`) must still compile and match.
- Context-window detection runs over the **full** error body available at `src/provider/http.rs:192-200` (already capped at `MAX_ERROR_BODY_BYTES` = 4096), **before** the 200-char truncation in `bad_request_detail` (`src/provider/http.rs:248-265`), and must consider sibling `error.code` and `error.type`, not only `error.message`.
- Everything new on the transport and CLI path stays behind `#[cfg(feature = "cli")]` where its neighbours already are; `cargo build --lib --no-default-features` must still build.

**Must-not**:
- Do not make the transport async or introduce tokio (ADR-001 Decision 2).
- Do not use `cliclack` on the commit path.
- Do not change `DEFAULT_TIMEOUT_SECS`, `MODEL_FETCH_TIMEOUT`, `DEFAULT_MAX_RETRIES`, the backoff constants, or `is_retryable`. Making `Timeout` retryable is **out of scope** - this task makes the existing behaviour visible, it does not change it.
- Do not put the CLI-side diagnostic helpers in the library. `src/provider/facade.rs:23` re-exports from the `gcm` crate, so binary and library are **separate crates** and `pub(crate)` does not bridge them. Making the helper `pub` in the library instead would enlarge the public surface for no consumer.
- Do not fix the oversized prompt itself. Shrinking the grouping prompt is CLO-797.
- Do not add a new CLI flag. Visibility is default-on; `--json` is the only modifier.
- Do not add a runtime dependency. `console` is already present; prefer `std::io::IsTerminal`, already used at `src/ui.rs:1`.

**Prefer**:
- Put the ticker in `src/ui.rs` beside the other stderr-facing helpers, as a guard value whose `finish()`/`Drop` stops the thread.
- Make `http::timeout_secs()` `pub` (additive) rather than duplicating the 60s default binary-side.
- Carry the context-window verdict from the transport to the binary as a **canonical marker prefix** on the existing `BadRequest { detail }` string (e.g. `context_window: <detail>`), which the binary strips when composing prose. This is the only channel that does not change `ErrorKind`'s shape, and it survives truncation because the marker is prepended after detection.
- Route the existing bare `eprintln!` calls on the generation path (`src/main.rs:429` curated-index warning, `src/main.rs:903` transition announcement) through the same coordinated emitter, so they cannot corrupt a live progress line either.
- Frame sizes with a small `human_bytes`-style helper (`~48 KB`) in user-facing text; keep exact bytes in the debug lines.

**Escalate when**:
- A provider's context-window rejection cannot be distinguished from a genuine malformed-request 400 without matching text that also appears in unrelated errors. In particular, **do not** match Gemini on the bare phrase `exceeds the maximum`; require a token-window-specific anchor (e.g. `input token count` together with `exceeds`, or `code` = `INVALID_ARGUMENT` plus a token-count phrase). If no safe anchor exists for a provider, leave that provider undetected and say so in the PR rather than producing false positives.
- Making progress visible would require changing the `Provider` trait signature (it should not: all four call sites in `main.rs` have everything needed).
- The coordinated emitter cannot prevent interleaving in some path, risking corrupted output.

## 4. Decomposition

1. **Default log level + `warn_log!`** - change `log_level()` (`src/debug.rs:39-48`) to default `Warn` instead of `Off`; unparseable `GCM_LOG_LEVEL` falls back to `Warn`, not `Off`; `GCM_DEBUG` legacy behaviour unchanged. Add `warn_log!` beside `debug_log!` (`src/debug.rs:73-79`). Files: `src/debug.rs`.
2. **Coordinated stderr emitter** - in `gcm::debug` (library, single static): an "a progress line is live, N columns wide" cell plus `emit_line()` that clears the line before writing and marks it needing redraw. Rewire the `log!` macro (`src/debug.rs:63-70`) through it. Files: `src/debug.rs`.
3. **Retry notice at warn** - `retry_with` (`src/provider/http.rs:315-340`): promote to `warn_log!`, reword to `attempt N of M, retrying in Xs`, include the provider and the classified kind. Files: `src/provider/http.rs`.
4. **Context-window signal survives extraction** - a pure library-side detector run over the full error body inside `send_once`/`get_once` before truncation, considering `error.message`, `error.code` and `error.type`; on a hit, `bad_request_detail` returns the detail prefixed with the canonical marker. Make `timeout_secs()` `pub`. Files: `src/provider/http.rs`.
5. **Prompt-size accounting** - `GroupingContext::section_sizes()` returning the four section byte counts plus total, and the equivalent for `GatheredDiff`; emit them via `debug_log!` where the context is built. Files: `src/diff.rs:98-114`, `src/main.rs:543-556`.
6. **`ui::CallProgress`** - the status line (operation, provider, model, file count, `~N KB`) plus a ticker thread with an interruptible wait: `\r` redraw every 100ms on a TTY, a plain `still waiting... Ns` line every 5s otherwise, inert under `--json`, registered with the coordination cell from (2). Files: `src/ui.rs`.
7. **Binary-side diagnostics + wiring** - a new private `src/provider/diagnostics.rs` (declared in `src/provider/facade.rs:16-21`) composing the context-window message (operation-specific advice per the section 1 table) and the timeout message (covering `ErrorKind::Timeout` **and** `ErrorKind::Transport` containing `timeout`, naming the budget that actually applied). Wire `CallProgress` and the diagnostics into all four call sites (`src/main.rs:553`, `:620`, `:782`, `:892-906`) and make the transition announcement unconditional (drop the `!args.json` gate at `src/main.rs:903`). Files: `src/provider/diagnostics.rs` (new), `src/provider/facade.rs`, `src/main.rs`.

**Dependency order**: 1 -> 2 (2 rewires the macro 1 adds). 3 depends on 1 and 2. 4 and 5 are independent of everything. 6 depends on 2. 7 depends on 4, 5 and 6. Suggested order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7.

## 5. Evaluation

Integration tests follow the existing convention in `tests/provider.rs` and `tests/resolve_integration.rs`: drive the built `gcm` binary as a subprocess against a throwaway git repo, a private `GCM_CONFIG`, a cleared provider environment, and a `std::net::TcpListener` stub on `127.0.0.1:0` wired in through `GCM_GROQ_BASE_URL`. New integration tests go in a single `tests/observability.rs`.

| # | Test | Expected Result | How to Run | AC |
|---|------|-----------------|------------|-----|
| 1 | Level table: `GCM_LOG_LEVEL` unset / `off` / `error` / `warn` / `bogus` / `GCM_DEBUG=1` | `enabled(Warn)` is true, false, false, true, true, true respectively | `cargo test --lib log_level_defaults_to_warn` | AC-3 |
| 2 | Stub accepts, sleeps 12s, then replies 200; run with `GCM_HTTP_TIMEOUT_SECS=30`, stderr piped | First stderr line < 2s; no gap between consecutive writes > 5s | `cargo test --test observability slow_endpoint_emits_progress` | AC-1, AC-8 |
| 3 | Same run, first stderr line | Names operation `grouping`, provider, model, `N files`, `~` + size | `cargo test --test observability status_line_names_operation` | AC-2 |
| 4 | Stub replies 429 twice then 200; `GCM_RETRY_BASE_MS=10`, no log env set | stderr carries `attempt 1 of 4` and `attempt 2 of 4` with delays; exit 0 | `cargo test --test observability retry_notices_visible_by_default` | AC-3 |
| 5 | Coordination: a retry notice fires while a ticker line is live (in-process, against an in-memory writer) | The clear sequence precedes the notice, the notice is written whole, the ticker redraws after | `cargo test --lib emit_line_clears_progress_first` | AC-10 |
| 6 | Timeout phase A: stub accepts and never sends headers, `GCM_HTTP_TIMEOUT_SECS=1` | Message names `1s` and `GCM_HTTP_TIMEOUT_SECS` | `cargo test --test observability timeout_before_headers` | AC-4 |
| 7 | Timeout phase B: stub sends `200` headers then stalls the body, `GCM_HTTP_TIMEOUT_SECS=1` | Classified `Transport("timeout...")` yet still produces the same budget-naming message | `cargo test --test observability timeout_mid_body` | AC-4 |
| 8 | Wizard path: model-list fetch times out against a stalling stub | Message does not claim the 60s generation budget | `cargo test --test observability model_fetch_timeout_uses_own_budget` | AC-4 |
| 9 | Detector table over real 400 bodies: signal in `error.message`; signal only in sibling `error.code`; signal past char 200; three unrelated 400s; a bare `exceeds the maximum` with no token anchor | true, true, true, false x3, **false** (no false positive) | `cargo test --lib context_window_detector` | AC-5 |
| 10 | End-to-end: stub replies 400 with a Groq `context_length_exceeded` body, run reaches the grouping call | stderr names cause, file count, prompt size, suggests `--all`; contains neither "gcm bug" nor "please report it" | `cargo test --test observability context_window_message_grouping` | AC-5 |
| 11 | Same stub, but the run is on the `--all` single-commit path | Advice is *not* `--all`; suggests smaller batches or `GCM_DIFF_TOTAL_BYTES` | `cargo test --test observability context_window_advice_is_operation_specific` | AC-5 |
| 12 | `GCM_LOG_LEVEL=debug` on a repo with staged changes | One debug line per section (`file_list`, `status`, `stat`, `body`) with byte counts, plus a total | `cargo test --test observability debug_logs_prompt_sections` | AC-6 |
| 13 | Capture `--json` stdout for `plan`, `noop`, `committed`, `fallback`, `error`; compare against committed golden envelopes | Shape, `v`, `status`, `mode`, `error.code`, `fallback.raw_code` identical; only `error.message`/`fallback.reason` differ | `cargo test --test observability json_contract_frozen` | AC-7 |
| 14 | Grouping fails, run falls through to single-commit, under `--json` | stderr announces the transition and its reason; stdout is one valid `fallback` envelope | `cargo test --test observability transition_announced_under_json` | AC-11 |
| 15 | Stderr redirected to a file (non-TTY) | No `\x1b[` and no `\r`; progress appears as whole lines | `cargo test --test observability non_tty_writes_plain_lines` | AC-8 |
| 16 | Immediate failure: `GROQ_API_KEY` unset so `MissingKey` is raised inside `generate_plan` | Clean exit, no ticker fragment, no `\r`, wall-clock under 1s (proves the non-TTY wait is interruptible) | `cargo test --test observability fast_failure_leaves_no_ticker` | AC-12 |
| 17 | Public surface gate | Script exits 0; `cargo doc --lib --no-default-features` builds; `tests/library_provider_api.rs` passes unchanged | `scripts/check-public-surface.sh && cargo test --test library_provider_api` | AC-9 |
| 18 | Full suite + lints | `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` clean, all tests pass | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` | all |

**Manual verification** (following the existing precedent in `tests/provider.rs:8-10`, where the cliclack wizard is verified by hand): on a real TTY, run against a stub that replies 429 twice then stalls, and confirm by eye that the retry notices appear as whole lines, the ticker resumes below them, and the final line leaves the cursor at column 0. A PTY harness would need a new dev-dependency; test 5 covers the sequencing deterministically instead.

**Edge cases to verify**:
- **Fast call**: a provider answering in 150ms leaves no ticker line and no dangling `\r`.
- **Non-UTF-8 or truncated body**: the detector runs on a body already capped at 4096 bytes and must tolerate a phrase cut mid-word without panicking.
- **Ollama (local, no auth)**: the status line renders with no API key present and names no env var.
- **Zero changed files**: the generation path is never reached (`src/main.rs:391-396` returns `noop`); no status line appears.
- **Ctrl-C mid-call**: the process dies with the ticker thread. Do not hide the cursor at all, so there is nothing to restore.
- **Two calls in one run**: grouping fails, fallback message succeeds - two status lines with *different* operation labels, separated by the transition announcement.

## Findings that reshaped this spec (v1 -> v2)

Each was reproduced against the tree before being accepted.

| # | Finding | Evidence | Resolution |
|---|---------|----------|------------|
| 1 | Byte-identical `--json` conflicts with changing error prose | `src/output.rs:194` serializes `err.to_string()` into `error.message`; `:180` serializes `reason` | AC-7 rewritten: schema + codes frozen, prose free (decision 3) |
| 2 | A `pub(crate)` helper in the library is unreachable from the CLI | `src/provider/facade.rs:23` re-exports from the `gcm` crate - separate crates | Diagnostics move to a private binary module, sub-task 7 |
| 3 | Promoting retries to `Warn` emits nothing | `log_level()` returns `Off` when both vars are unset (`src/debug.rs:39-48`); `enabled(Warn)` is false | Sub-task 1 changes the default to `Warn`; AC-3 pins the whole level table |
| 4 | The detector would see less than assumed | `bad_request_detail` (`src/provider/http.rs:248-265`) takes only `error.message` and truncates to 200 chars, discarding sibling `code` | Detection moves before truncation and considers `code`/`type`; marker prefix carries the verdict (sub-task 4) |
| 5 | A `Display`-only timeout fix is wrong twice | `.send()` errors map to `Timeout` but a 2xx body stall maps to `Transport` (`src/provider/http.rs:177-181`); `get_once` shares `ErrorKind::Timeout` with a fixed 5s budget (`:37,98`) | AC-4 covers both phases and the wizard budget; message composed binary-side (decision 5) |
| 6 | The ticker-lifecycle test tested the wrong thing | `MissingKey` is raised **inside** `generate_plan` (`src/provider/groq.rs:62`), so the guard is already live | AC-12 reframed as "fails fast and cleans up", plus the interruptible-wait requirement |
| 7 | Fallback can stay unexplained and the advice can loop | Transition announcement gated on `!args.json` (`src/main.rs:903`); `--all` is useless advice on the path that already is `--all` | AC-11 plus the operation table in section 1; sub-task 7 covers all four call sites |
| 8 | `src/debug.rs` compiles into both crates | `src/lib.rs:32` `pub mod debug` and `src/main.rs:5` `mod debug` | Constraint: the coordination flag is a single static in `gcm::debug` |

## Out of scope

- Shrinking the grouping prompt so it fits the context window - **CLO-797**.
- Making `ErrorKind::Timeout` retryable, or changing any retry/timeout default.
- A `--quiet` or `--verbose` flag.
- Progress for the `gcm resolve` provider calls. The gap is genuinely larger there: `src/resolve/mod.rs:966` calls `resolve_hunks` inside a `for batch in batches` loop (plus a second call site at `src/resolve/mod.rs:1151`), so a resolve run makes N sequential blocking calls with per-batch progress semantics. `ui::CallProgress` is built to be reusable there; **file a follow-up issue** once this lands.
