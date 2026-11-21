# Pre-PR Validation: clo-805

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Validated**: 2026-11-21
**Pipeline**: implement.md validation gate

---

## Verdict: PASS_WITH_NOTES

All changes match `/Users/mk/Code/gcm--fix-clo-805-enable-models/specs/2026-11-21-clo-805-enable-models-unchanged-confirm.md`. No CRITICAL, HIGH, or MEDIUM issues were found: the confirm loop, canonical set comparison, RAII theme scoping, `README.md` update, and `expect`-based PTY cases cleanly satisfy AC1-AC10 and all Must / Must-not constraints without altering the public library surface.

## Findings

- **LOW - Inactive-checked row state omitted from unit test matrix** (`/Users/mk/Code/gcm--fix-clo-805-enable-models/src/config/facade.rs:866`)
  `ROW_KINDS` covers `(false, true)`, `(true, true)`, and `(false, false)`, leaving `(true, false)` (checked and not highlighted) untested in `space_hint_only_on_live_active_unchecked_row` and `checkbox_item_matches_default_except_hinted_row`. The implementation (`active && !selected` at line 520) handles it correctly, so this is only a test-matrix gap.

- **LOW - PTY regression cases do not assert the `SPACE to select` hint in live terminal output** (`/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1634-1663`)
  AC7 is verified by unit tests against `DefaultTheme`, and AC9 does not strictly mandate asserting the hint text over the PTY. However, adding a PTY check for `SPACE to select` when `oss-20` filters to the unchecked `openai/gpt-oss-20b` row would also verify end-to-end that `ThemeGuard` installs `EnableModelsTheme` during `multiselect::interact()`.

- **LOW - Tracking phase stale in project dashboard** (`/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/PROJECT.md:66`)
  The Active Work row for CLO-805 still lists `Phase` as `Spec`, whereas implementation is complete and `/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/status/clo-805-workflow.yaml:386` records `implement`.

## Missing Items

None. All acceptance criteria (AC1-AC10) are implemented and covered by unit tests (`/Users/mk/Code/gcm--fix-clo-805-enable-models/src/config/facade.rs:806-912`), PTY cases (`/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1598-1737`), and documentation (`/Users/mk/Code/gcm--fix-clo-805-enable-models/README.md:210-213`).

## Recommendations

1. Expand `ROW_KINDS` in `/Users/mk/Code/gcm--fix-clo-805-enable-models/src/config/facade.rs:866` to include `(true, false)` so all four `(selected, active)` combinations are asserted across all four `ThemeState` variants.
2. In `/Users/mk/Code/gcm--fix-clo-805-enable-models/scripts/acceptance.sh:1637` (or inside `case no` before `send " "`), assert that `SPACE to select` appears on the filtered unchecked row so `EnableModelsTheme` rendering is covered end-to-end in the PTY suite as well.
3. Update `Phase` from `Spec` to `Implement` (or `Review`) in `/Users/mk/Code/gcm--fix-clo-805-enable-models/docs/PROJECT.md:66` when syncing status before merge.
