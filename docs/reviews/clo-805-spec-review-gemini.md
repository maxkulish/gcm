# Spec Review: clo-805

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Reviewed**: 2026-11-21
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

Clear, accurate, and directly aligned with the Linear issue (**CLO-805**).

- **Codebase accuracy**: File paths and symbols match the repo (`run_provider_wizard` at `src/config/facade.rs:23`, `canonicalize_model` at `src/config/facade.rs:218-226`, `note` + `multiselect` at `src/config/facade.rs:231-248`, and `scripts/acceptance.sh:403-425`).
- **Scope framing**: The `cliclack 0.5.4` private-module constraint (`FilteredView` / `PromptInteraction`) justifies why "ENTER acts as SPACE" was rejected in favor of the owner-approved **"Confirm + hint"** design, which satisfies the Linear AC branch *"makes it unmistakable before exit that nothing changed"*.
- **Minor nuance**: Line range `src/config/facade.rs:226-247` starts on the closing brace of `initial_enabled`; the actual `note` + `multiselect` block spans `231-248`.

---

## 2. Acceptance Criteria Review

**Strong**:
- **AC1, AC3, AC4, AC5, AC6**: Specific, testable, and preserve existing invariants (CLO-799 `note()`, CLO-802 newly-enabled-wins-default rule, `required(true)` on first-time setup, Esc/Ctrl-C cancellation via `wizard_cancelled()`).
- **AC8**: Clear scoping requirement that no other `cliclack` prompt (`Provider` select, `Default model` select, `confirm`) inherits the multiselect theme.
- **AC9**: Directly encodes the Linear PTY regression requirement for both the "No -> SPACE -> save new" and "Yes -> keep old" paths.

**Gaps**:
- **AC2 vs. Decomposition #1 ambiguity on `note()`**: AC2 states *"Answering No reopens the Enable-models multiselect with the same candidates, the same checked set, the same `note()` and the same title."* However, Decomposition #1 proposes wrapping `src/config/facade.rs:236-247` in a loop—leaving `note(...)` (`lines 231-236`) *outside* the loop. Because `cliclack` prints the collapsed multiselect and `confirm` answer between the initial `note` and the reopened prompt, clarify whether `note()` sits inside the loop (re-printed on "No") or outside (printed once).
- **AC4 / AC9 additive check**: AC9 asserts on the "No" path that *"the saved config enables the new model and makes it the default"*, which does not explicitly require verifying that the previously enabled model `A` **remains** in `models` (verifying *"additive SPACE behavior unchanged"* from the Linear description).
- **AC7 example wording**: AC7 gives `(recommended; SPACE to select)` as an example, whereas `gcm`'s `wizard_model_hint` (`src/config/mod.rs:715-732`) only emits `""` or `"not in live catalog"`. Using `(not in live catalog; SPACE to select)` avoids misleading an implementer into looking for a `"recommended"` hint.

---

## 3. Constraints Check

**Aligned**:
- **Public library boundary**: Keeping helpers in binary-only `src/config/facade.rs` and locking `src/config/mod.rs` aligns with `docs/adrs/002-library-boundary.md` (Decisions 3 & 6) and `scripts/check-public-surface.sh`.
- **Canonical comparison**: Reusing `canonicalize_model` (`src/config/mod.rs:519-526`) avoids false mismatches on Ollama `:latest` or Gemini `models/` entries.
- **Zero new dependencies / skip guard**: Matching `scripts/acceptance.sh`'s `expect` pattern avoids adding Rust `[dev-dependencies]`.

**Concerns**:
- **Contradiction between Constraint (line 59) and Decomposition #3 (line 70)**: Constraint line 59 says *"let the live model fetch fail against the mock (the mock serves no GET route) so that the wizard uses the built-in fallback list"*, while Decomposition #3 says *"pass the key step"*. In `run_provider_wizard` (`src/config/facade.rs:65-91`), a key prompt only appears when **no** key is in `config.toml` or the environment—and if the user skips that prompt, `fetch_supported_models_with` (`src/provider/models.rs:88-98`) short-circuits **before** making any HTTP request to the mock. Pick one deterministic offline path:
  - Either seed `key = "dummy"` in `config.toml` + `GCM_*_BASE_URL="$MOCK_URL"` (no key prompt; mock returns HTTP 501 on GET, `http::get_json` retries once after 200ms and falls back), **or**
  - Omit `key` from `config.toml` + unset the provider key env var + skip the password prompt with `\r` (triggers the no-key short-circuit with zero HTTP calls).
- **Missing `cliclack 0.5.4` verification check**: The constraint assumes `cliclack::reset_theme()` and `Theme::{checkbox_symbol, checkbox_style, placeholder_style}` are public in `0.5.4`. If `reset_theme` has a different name or `Theme` stores global `OnceLock`/`Mutex` state, flag verifying those signatures at compile time (and note that `ThemeGuard` RAII drop is safer than manual `reset_theme()`).
- **Missing environment-isolation constraint for PTY tests**: `scripts/acceptance.sh` does **not** globally unset `GROQ_API_KEY` / `OPENAI_API_KEY` / `GCM_PROVIDER`. The PTY test must explicitly unset the chosen provider's key env var and `GCM_PROVIDER` (or set `GCM_*_BASE_URL="$MOCK_URL"`), matching `AC-ONB3` (`scripts/acceptance.sh:1575`).

---

## 4. Decomposition Quality

**Well-scoped**:
- Three small, sequential/parallelizable sub-tasks confined to `src/config/facade.rs` and `scripts/acceptance.sh`, each well under 2 hours.
- Dependency order (`1` and `2` independent; `3` after `1` and `2`) is accurate.

**Issues**:
- **Sub-task 2 (`space_hint` signature vs. AC7)**: `fn space_hint(hint: &str, selected: bool, active: bool) -> String` omits the prompt's `ThemeState` (or a `bool` representing `Active | Error` vs. `Submit | Cancel`). AC7 explicitly requires the hint only in `Active` or `Error` state and *not* in `Submit` or `Cancel`. If the state gate is left outside `space_hint`, unit tests on `space_hint` cannot verify the `Submit`/`Cancel` requirement.
- **Sub-task 3 (`config.toml` permissions & cleanup)**:
  - `load_from` (`src/config/mod.rs:230-238`) silently **ignores** any `config.toml` whose mode is wider than `0600`. Sub-task 3 must `chmod 600` the seeded `config.toml` (as `tests/provider.rs:53` does), otherwise `load()` returns `None` and the confirm prompt never triggers.
  - Sub-task 3 must create and clean up its own `mktemp -d` `GCM_CONFIG` directory (matching `onb_cfg` in `scripts/acceptance.sh:1540`) rather than reusing the suite-wide `$GCM_CONFIG`.
  - On Case "no", assert `models` contains **both** `A` and `B` (not just `B`), and check `expect eof` + exit code `0` before inspecting `config.toml`.
- **Sub-task 1 (`initial_enabled` ownership across loop iterations)**: Minor Rust detail—`multiselect::initial_values` takes `Vec<String>` by value, so looping requires `initial_enabled.clone()`.

---

## 5. Evaluation Coverage

**Covered**:
- Pure logic for `enabled_set_unchanged` (equal, reordered, canonical-equal, added, removed, empty initial) and `unchanged_confirm_message` (singular/plural) is well covered by unit tests (rows 1–3).
- Full suite (`make check`) and library boundary (`scripts/check-public-surface.sh`) are included (rows 8–9).
- Esc cancellation and post-multiselect radio rendering are covered manually (rows 10–11).

**Gaps**:
- **Rows 4–5 (`space_hint`)**: Do not test `ThemeState::Submit` or `ThemeState::Cancel` (AC7).
- **Row 6 (PTY "No" case)**: Only checks `models` holds `B` and `model = B`; should also assert `A` is still in `models` (verifying additive SPACE behavior) and that `gcm provider` exits `0`.
- **Missing test case for `confirm` + Ctrl-C / Esc or `No` -> `Yes`**: Edge case #4 mentions repeating "No" twice without stacking themes, which is not in the Evaluation table (worth adding to manual check #10/#11 or the PTY "no" case).
- **Note on `scripts/acceptance.sh` gating**: Confirm that the new PTY cases gate only on `command -v expect` (not `SIGNING_OK`, since `gcm provider` does no git commits), and run inside `start_mock`..`stop_mock` if hitting `$MOCK_URL`.

---

## 6. Codebase Alignment

*(Note: Review criterion #6 references `Backend` trait, `anyhow`, and `BackendErrorKind` from another project template. This codebase (`gcm`) uses the binary-only `Provider` trait in `src/provider/facade.rs:46`, `GcmError` in `src/error.rs:10`, and `ProviderError`/`ErrorKind` in `src/provider/identity.rs:16,74`.)*

**Alignment**:
- **Library boundary (ADR-002)**: Correctly leaves `Provider` trait, `src/config/mod.rs`, `src/provider/`, and `GcmError` untouched, confining all `cliclack` code and pure wizard helpers to binary-only `src/config/facade.rs`.
- **Error & cancel handling**: Uses `wizard_io` and `wizard_cancelled()` (`Ok(false)` -> exit `1` in `src/main.rs:258-265`), matching every other prompt in `run_provider_wizard`.
- **CLO-799 / CLO-802 / CLO-800 invariants**: Preserves `wizard_model_list`, `initial_default_model`, `build_provider_config`, and `merge_provider_config` without regression.

**Violations**:
- None found against `gcm` patterns, though Sub-task 2 proposes manual `set_theme` / `reset_theme` pairs whereas `src/config/facade.rs:489-496` already establishes an RAII guard pattern (`EchoGuard`) for terminal state restoration.

---

## 7. Blind Spots

1. **Seeded `config.toml` mode `0600` in `acceptance.sh`**: Without `chmod 600`, `config::load()` rejects the seeded file with an insecure-permissions warning and falls into first-run mode (`initial_enabled` empty), causing the PTY test to fail mysteriously or test the wrong branch.
2. **Developer env leakage in PTY tests**: An exported `GROQ_API_KEY` or `OPENAI_API_KEY` in the runner's shell skips the password prompt and triggers a live network call unless explicitly unset (`env -u ...`) or redirected to `$MOCK_URL`.
3. **Panic/unwind theme leak**: In debug/test builds (`panic = "unwind"`), calling `reset_theme()` manually after `interact()` leaves `EnableModelsTheme` installed if a panic occurs inside `interact()`. A small `ThemeGuard` struct with `Drop` solves this cleanly.
4. **Sequential `expect` matching on `cliclack` ANSI output**: `cliclack` renders cursor/highlight changes using ANSI escape sequences and carriage returns; the spec does not mention how the `expect` script synchronizes between typing the filter, waiting for the filtered row, and sending SPACE/ENTER (sending SPACE too early before the filter redraws could toggle the wrong row).
5. **Documentation sync**: `README.md:208-210` ("Selecting which models to use (`gcm provider`)") describes the CLO-799 note and `enter` saving directly; check whether a brief update is desired or explicitly out of scope.

---

## 8. Verdict

`APPROVE_WITH_SUGGESTIONS`

---

## 9. Actionable Feedback

1. **[High] Include prompt state in `space_hint` (Sub-task 2 & Evaluation #4-#5)**: Pass `state: &ThemeState` (or `is_active: bool` derived from `matches!(state, ThemeState::Active | ThemeState::Error(_))`) into `space_hint` and add unit tests verifying that `Submit` and `Cancel` states return the original hint unchanged (AC7).
2. **[High] Resolve the mock-vs-key-step contradiction and add PTY test hygiene (Constraint line 59, Sub-task 3, Evaluation #6-#7)**:
   - Pick either `key = "dummy"` in config + `GCM_*_BASE_URL="$MOCK_URL"` (no key prompt; 501 fallback) **or** no key + `env -u <KEY_VAR>` + `\r` at the key prompt (no-key short-circuit).
   - Explicitly specify `chmod 600` on the seeded `config.toml`, `env -u <KEY_VAR> -u GCM_PROVIDER`, an isolated `mktemp -d` `GCM_CONFIG` cleaned up per case, and waiting for `eof` + exit code `0`.
   - In Case "no" (AC9 / Evaluation #6), assert that `models` contains **both** `A` and `B` (verifying additive SPACE behavior per AC4) and `model = B`.
3. **[Medium] Clarify `note()` placement in the "No" loop (AC2 vs. Sub-task 1)**: Decide whether `note("Enable models", ...)` (`src/config/facade.rs:231-236`) stays before the loop or moves inside it so AC2 and Sub-task 1 agree.
4. **[Medium] Use an RAII `ThemeGuard` (Sub-task 2)**: Mirror `EchoGuard` (`src/config/facade.rs:489`) so `cliclack::reset_theme()` runs on `Drop` across submit, cancel, error, and unwind paths; verify `cliclack::reset_theme` and `Theme` helper method signatures against `cliclack 0.5.4` at compile time.
5. **[Low] Fix AC7 hint example**: Replace `(recommended; SPACE to select)` with `(not in live catalog; SPACE to select)` to match `wizard_model_hint`. Note whether `README.md:208-210` is in or out of scope.
