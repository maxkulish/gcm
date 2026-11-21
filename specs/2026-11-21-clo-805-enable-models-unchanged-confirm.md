# Spec: Confirm an unchanged Enable-models set and hint SPACE on the highlighted row

**Created**: 2026-11-21 (revised after the Gemini + Codex spec review, `docs/reviews/clo-805-spec-review-synthesis.md`)
**Linear**: [CLO-805](https://linear.app/cloud-ai/issue/CLO-805)
**Estimated scope**: S (3 files, 3 sub-tasks)

## 1. Problem Statement

`gcm provider` runs an interactive wizard (`run_provider_wizard`, `src/config/facade.rs:23`). Step 4 is the **Enable models** multiselect (`src/config/facade.rs:231-248`): a cliclack 0.5.4 `multiselect` with `filter_mode()`, `required(true)`, `max_rows(15)` and `initial_values(initial_enabled)`, where `initial_enabled` holds the candidates that match the provider's saved `models` list. Step 5 is the **Default model** radio, built only from the models returned by step 4.

In the multiselect, typing filters the visible rows, and only SPACE checks or unchecks a row. ENTER submits the checked set across all rows, visible or not. A user who filters the list to the model they want and presses ENTER without SPACE submits the set that was already checked. Often that set holds a stale model which the provider no longer serves. The Default-model radio then offers only that stale model, and `Saved ... configuration` prints with no warning. The same wizard's Provider and Default-model prompts are single-select, and there type-to-filter + ENTER does choose the row. That is why the multiselect behavior surprises users.

CLO-799 added a `note()` above the list ("Filtering alone does NOT change the selection") and a prompt title that names SPACE and ENTER. A third user report (2026-09-18, reproduced byte-for-byte in a PTY against a real config: filter `deepseek`, ENTER, the stale `nemotron-3-nano:30b-cloud` was saved as both `models` and `model`) showed that a static warning does not stop this mistake.

Constraint found while scoping: cliclack 0.5.4 keeps the filter input and the filtered rows in a private `FilteredView` (`cliclack-0.5.4/src/filter.rs`), and `PromptInteraction` lives in a private `mod prompt`. Code outside cliclack therefore cannot see the filter string, and it cannot change what ENTER does inside the multiselect. Two public hooks remain: the value that `interact()` returns, and the global `Theme` (`cliclack::set_theme` / `reset_theme`, trait `cliclack::Theme`, method `checkbox_item`, which only the multiselect calls).

**Chosen fix** (owner decision, 2026-11-21: "Confirm + hint"):

1. **Confirm on an unchanged set.** If the provider already had enabled models and the multiselect returns exactly that set, the wizard shows a `confirm` prompt that names the models which will stay enabled. "Yes" continues to the Default-model step. "No" reopens the multiselect.
2. **SPACE hint on the highlighted unchecked row.** While the multiselect is active, the row under the cursor shows `SPACE to select` in its hint when that row is not checked.

Ticket options rejected: (B) "ENTER acts as SPACE on a filter narrowed to one row" and filter-aware (A). Both need a vendored copy of the cliclack multiselect (about 400 lines of prompt, filter, view and terminal code), and B can surprise a user who filtered only to look.

**Product decision recorded - confirm default is Yes.** A user who re-runs the wizard only to rotate a key or change the default model passes with one extra ENTER. A default of No would loop that user between the list and the confirm prompt until they type `y`. Consequence, accepted: a user who presses ENTER twice on reflex still keeps the old set, but the prompt between the two presses names the kept models. Evaluation row 7 tests this path explicitly.

## 2. Acceptance Criteria

- [ ] **AC1 - unchanged set asks.** If `initial_enabled` is non-empty and the multiselect returns a set equal to `initial_enabled` (compared with `canonicalize_model(id, _)`, order ignored), the wizard shows a `confirm` prompt before the Default-model step. The prompt text starts with `Nothing changed` and names every model in the returned set.
- [ ] **AC2 - "No" reopens the list.** Answering No reopens the Enable-models multiselect with the same candidates and the same checked set (`initial_enabled`), the same title, an empty filter and the cursor on the first row (cliclack's fresh-prompt state). The CLO-799 `note()` is printed once, before the first multiselect, and is not repeated on a reopen. A new submit goes through the AC1 check again.
- [ ] **AC3 - "Yes" keeps today's behavior.** Answering Yes (or pressing ENTER on the default Yes) continues to the Default-model step with the returned set. The saved config is identical to what the wizard saves today for the same keys.
- [ ] **AC4 - a changed set does not ask.** If the returned set differs from `initial_enabled` in any member (SPACE added a model, removed a model, or both), no confirm prompt appears, and the flow goes straight to the Default-model step. The additive multiselect behavior and the CLO-802 newly-enabled-wins-default rule are unchanged.
- [ ] **AC5 - first-time setup does not ask.** If `initial_enabled` is empty, no confirm prompt appears. (`required(true)` already rejects an empty submit with `Input required`.)
- [ ] **AC6 - cancel stays cancel.** Any `Err` from the confirm prompt (Esc, Ctrl-C) goes to `wizard_cancelled()`, the same `Err(_) => return wizard_cancelled()` mapping that every other prompt in `run_provider_wizard` uses (`src/config/facade.rs:47,86,107,139,149,247,266`). The config file is unchanged afterwards (bytes and mtime). In the PTY cases, Esc at the confirm exits with code 1, and Ctrl-C at the confirm terminates the process by SIGINT (Unix).
- [ ] **AC7 - SPACE hint.** While the Enable-models multiselect renders in state Active or Error, the highlighted row shows `SPACE to select` in its hint when it is unchecked. A non-empty model hint is kept and joined with `; ` (for example `(not in live catalog; SPACE to select)`). A checked highlighted row, a row that is not highlighted, and the Submit and Cancel renders are byte-identical to cliclack's default `checkbox_item` output. Scope: rows that cliclack renders. A known cliclack 0.5.4 paging limit (narrowing the filter after a scroll past 15 rows can leave the highlighted row outside the rendered page) is out of scope and is noted in a code comment.
- [ ] **AC8 - hint is scoped.** The theme that adds the hint is installed only for the duration of each Enable-models `interact()` call. An RAII guard (same pattern as `EchoGuard`, `src/config/facade.rs:489`) calls `reset_theme()` on drop, so submit, cancel, error and an unwinding panic all restore (an aborting panic runs no destructors and is out of scope) cliclack's default theme. No other prompt in the binary changes its look.
- [ ] **AC9 - PTY regression.** `expect`-driven cases in `scripts/acceptance.sh` run the exact trap (fixture and steps in Sub-task 3): with A enabled, filter to an unchecked model B, ENTER without SPACE. Case "no" asserts the `Nothing changed` prompt naming A, answers No, retypes the filter, waits for the redraw, presses SPACE and ENTER, sees `Default model` with no second confirm, presses ENTER without navigation, and asserts the saved `models` equals exactly `{A, B}` and `model = B`. Case "yes" presses ENTER on the default Yes, accepts the default, and asserts `models == [A]` and `model = A`. Case "esc" presses Esc on the confirm and asserts exit code 1. Case "no-twice-ctrl-c" answers No twice and sends Ctrl-C at the third confirm, and asserts SIGINT termination (Unix). Both cancellation cases assert that the config bytes and mtime are unchanged. Each case asserts `eof` and the expected exit status.
- [ ] **AC10 - README.** The `gcm provider` paragraph in `README.md:204-211` states that ENTER on an unchanged set asks for confirmation, and that the highlighted unchecked row shows the SPACE hint.

**Verification method**: unit tests in `src/config/facade.rs` (`mod tests`) for the helpers and the theme renderer (AC1, AC4, AC5, AC7); the PTY cases in `scripts/acceptance.sh` against a fresh release build, run non-skipped (AC1-AC3, AC6, AC9); a baseline run of the "no" case against a pre-fix build, where the filtered ENTER reaches `Default model` instead of `Nothing changed`; `make check` green; `scripts/check-public-surface.sh` green.

## 3. Constraints

**Must**:
- Use only the public cliclack 0.5.4 API: `multiselect`, `confirm`, `set_theme`, `reset_theme`, `ThemeState`, and `Theme` trait methods. All of these exist in `cliclack-0.5.4/src/lib.rs:314-326` and `src/theme.rs:108-483`.
- Compare sets with `canonicalize_model(id, _)`, the comparison that already builds `initial_enabled` (`src/config/facade.rs:217-225`).
- Put the new helpers, the theme struct and the guard in `src/config/facade.rs`, which is binary-only. Unit-test them in its existing `mod tests` (`src/config/facade.rs:646`).
- Keep the CLO-799 `note()` text and the multiselect title as they are.
- Run the PTY cases non-skipped at least once during implementation, with `expect` installed and `GCM_BIN` pointing at a release binary built from the branch (`cargo build --release` first; `scripts/acceptance.sh:13,25` otherwise reuses any existing `target/release/gcm`). `make check` and CI do not run `acceptance.sh`, so record the run output in the PR body.
- In CI-less runs where `expect` is missing, skip the PTY cases with `skip "..."`, like the existing PTY cases (`scripts/acceptance.sh:403-424`).

**Must-not**:
- Vendor, fork or patch cliclack, or upgrade it in this change.
- Add a public item to the `gcm` library crate (`src/config/mod.rs` and others), because `scripts/check-public-surface.sh` locks that surface.
- Change what ENTER, SPACE or filter typing do inside the multiselect.
- Change `wizard_model_list`, `initial_default_model`, `build_provider_config` or `merge_provider_config`.
- Add a Rust dev-dependency for PTY testing. The repo has no `[dev-dependencies]`, and `expect` in `scripts/acceptance.sh` is the existing PTY pattern.
- Map the confirm prompt's errors differently from the sibling prompts.

**Prefer**:
- Wording: `Nothing changed: only <a>, <b> stay enabled. Keep them?` for several models, and `Nothing changed: only <a> stays enabled. Keep it?` for one model.
- Implement the hint by overriding only `Theme::checkbox_item` in a unit struct `EnableModelsTheme`. Rebuild the default body (`cliclack-0.5.4/src/theme.rs:430-460`) from the public trait methods `checkbox_symbol`, `checkbox_style` and `placeholder_style`, and change only the hint string through `space_hint`.
- Call `multiselect(..).initial_values(initial_enabled.clone())` inside the loop, because `initial_values` takes the `Vec` by value.

**Escalate when**:
- `checkbox_item` is not the only multiselect render path in cliclack 0.5.4 that the hint needs, or a theme override changes another prompt's output.
- The no-key Groq route cannot reach the multiselect under `expect` (for example, the key prompt rejects a single space or does not trim it to no key).
- On the pre-fix build, the filtered ENTER does not reach `Default model` (the baseline would then not show the bug).

## 4. Decomposition

1. **Theme and hint (no call-site change)**: add `fn space_hint(state: &ThemeState, hint: &str, selected: bool, active: bool) -> String` (returns the joined hint only for state Active or Error, active and unchecked; otherwise the original hint), `struct EnableModelsTheme;` with `impl cliclack::Theme` that overrides only `checkbox_item` and uses `space_hint`, and `struct ThemeGuard;` whose constructor calls `set_theme(EnableModelsTheme)` and whose `Drop` calls `reset_theme()`. Unit tests: `space_hint` for all four `ThemeState` values (Active, Error, Submit, Cancel) x {active unchecked, active checked, inactive}, with an empty and a non-empty hint; `EnableModelsTheme.checkbox_item(..)` equals the default `checkbox_item` output for every case except active-unchecked in Active/Error, where it contains `SPACE to select` (compare against a unit struct `DefaultTheme; impl Theme for DefaultTheme {}`). Add the paging-limit comment. Files: `src/config/facade.rs`.
2. **Confirm loop and call-site integration (owns the `interact()` call)**: add `fn enabled_set_unchanged(id, initial: &[String], selected: &[String]) -> bool` (false when `initial` is empty) and `fn unchanged_confirm_message(selected: &[String]) -> String`. Keep the `note()` before the loop. Inside a loop: create `let _theme = ThemeGuard::new();`, run the multiselect with `initial_values(initial_enabled.clone())`, drop the guard, map `Err(_)` to `wizard_cancelled()`. If `enabled_set_unchanged`, run `confirm(unchanged_confirm_message(&selected)).initial_value(true).interact()`: `Ok(true)` breaks with the set, `Ok(false)` continues the loop, `Err(_)` returns `wizard_cancelled()`. Otherwise break with the set. Unit tests: equal, reordered, canonical-equal (`llama3` vs `llama3:latest` for Ollama), added member, removed member, swapped member (same count), empty initial; message text for one and for two models. Update `README.md:204-211` (AC10). Files: `src/config/facade.rs`, `README.md`.
3. **PTY regression cases**: add four `expect` cases to `scripts/acceptance.sh`. Fixture, per case: a fresh `mktemp -d` dir as `GCM_CONFIG` (removed at the end of the case); a seeded `config.toml` with `version = 2`, `default = "groq"` and a Groq provider entry with no key, `models = ["<A>"]`, `model = "<A>"`, where A and B are two models from `static_fallback_models(Groq)` whose names do not share a filter substring; `chmod 600` on the seed (the loader ignores wider modes, `src/config/mod.rs:230`); run under `env -u GROQ_API_KEY -u GCM_PROVIDER` so the no-key short-circuit (`src/provider/models.rs:86`) gives the built-in list with no network call. Steps: choose Groq, at the key prompt send one space followed by ENTER (the wizard trims it to no key), wait for the `Enable models` title, type B's filter, wait for the redraw that shows B, press ENTER, expect `Nothing changed` and A. Case "no": send `n`, wait for the reopened `Enable models` title, retype B's filter, wait for the redraw, send SPACE then ENTER, expect `Default model` (and no `Nothing changed`), press ENTER without navigation, expect `eof`, assert `models` is exactly `{A, B}` and `model = "B"`. Case "yes": send ENTER, expect `Default model`, send ENTER, expect `eof`, assert `models == ["A"]` and `model = "A"`. Case "esc": send Esc at the confirm, expect `eof`, assert exit code 1. Case "no-twice-ctrl-c": answer `n` at the first and second confirm (reopening, retyping the filter and pressing ENTER each time), send Ctrl-C at the third confirm, expect `eof`, assert termination by SIGINT (Unix; read the signal from `expect`'s `wait` result). For both cancellation cases, record the config bytes and mtime before the run and assert both are unchanged after it. Baseline: run the "no" case once against a pre-fix release build (build `main` in a separate worktree and point `GCM_BIN` at its binary). After the filtered ENTER, assert that the pre-fix build reaches `Default model` where `Nothing changed` was expected, and record that result. Files: `scripts/acceptance.sh`.

**Dependency order**: Sub-task 1 is independent. Sub-task 2 uses `ThemeGuard` from Sub-task 1. Sub-task 3 runs against the build that contains 1 and 2.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `enabled_set_unchanged` for equal, reordered and canonical-equal sets | `true` | `cargo test --bin gcm enabled_set_unchanged` |
| 2 | `enabled_set_unchanged` for added, removed and swapped members, and an empty initial set | `false` | same |
| 3 | `unchanged_confirm_message` with 1 model and with 2 models | Starts with `Nothing changed`, names every model, singular/plural wording | `cargo test --bin gcm unchanged_confirm_message` |
| 4 | `space_hint` across 4 states x 3 row kinds x 2 hint values | Joined hint only for Active/Error + active + unchecked | `cargo test --bin gcm space_hint` |
| 5 | `EnableModelsTheme.checkbox_item` vs default `checkbox_item` | Identical except active-unchecked in Active/Error | `cargo test --bin gcm checkbox_item` |
| 6 | PTY "no": filter B, ENTER, `n`, retype filter, SPACE, ENTER, ENTER at Default model (no navigation) | Prompt names A; no second prompt; `models` exactly `{A, B}`, `model = B`; `eof`, exit 0 | `cargo build --release && bash scripts/acceptance.sh` |
| 7 | PTY "yes": filter B, ENTER, ENTER (default Yes), ENTER | Prompt names A; `models == [A]`, `model = A`; `eof`, exit 0 | same |
| 8 | PTY "esc": filter B, ENTER, Esc at confirm | Exit 1; config bytes and mtime unchanged; `eof` | same |
| 9 | PTY "no-twice-ctrl-c": No twice, Ctrl-C at the third confirm | SIGINT termination (Unix); config bytes and mtime unchanged; `eof` | same |
| 10 | Baseline: PTY "no" against a pre-fix build | The filtered ENTER reaches `Default model` where `Nothing changed` was expected | `GCM_BIN=<pre-fix binary> bash scripts/acceptance.sh` |
| 11 | Full unit and integration suite | All pass, no new warnings | `make check` |
| 12 | Public surface unchanged | Script passes | `scripts/check-public-surface.sh` |

**Edge cases to verify**:
- The saved `models` contain a stale model that the live catalog no longer lists. `wizard_model_list` keeps it as a candidate, so it is in `initial_enabled`, and the confirm prompt names it.
- The saved `models` list two models, and the user unchecks one and checks another (same count, different members). No prompt appears (AC4, unit test "swapped member").
- A user who toggles a row on and then off again before ENTER returns the original set. The prompt appears, which is correct, because the set did not change.
- No answered twice in a row reopens the list each time with `initial_enabled` checked, and the guard restores the default theme after each `interact()`.
