# Pre-PR validation: clo-535

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

## Re-validation

After applying the three Must Fix Before PR items in one bounded iteration:

1. **CRLF dangling `\r`**: terminal trim now removes a trailing `\r` when `uses_crlf` is true (`src/resolve/mod.rs:645-648`).
2. **CRLF test tightened**: `reconstruct_crlf_no_final_newline_preserved` asserts `!out.ends_with('\r')` and `assert_eq!(out, "resolved")`.
3. **Empty-replacement guard**: the newline guard is skipped for `text.is_empty()`; added `reconstruct_empty_resolution_no_extra_blank` regression test.

Verification re-run:
- `cargo fmt --check` ✓
- `cargo clippy --all-targets -- -D warnings` ✓
- `cargo test --bin gcm` → 336 passed, 0 failed
- `cargo test --tests` → onboarding (6), provider (5), resolve_integration (10), status (17) all passed

The original verdict of **PASS_WITH_NOTES** stands; all notes are addressed and the pre-merge gate is green.

---

Verified. The pre-merge gate is green in a writable environment: `cargo clippy --all-targets -- -D warnings` exits 0, and all 6 `reconstruct_` tests pass — including `reconstruct_crlf_no_final_newline_preserved`, which passes while its output is the malformed `"resolved\r"`. That green-but-wrong test is direct proof of the CRLF finding both reviewers raised.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Verdict FAIL; 3 findings. CRLF finding + trailing-whitespace confirmed against code; empty-replacement confirmed reachable but narrower than described. Could not run clippy/test (read-only session) — I closed that gap. |
| Gemini | OK | Verdict PASS_WITH_NOTES; F1 (CRLF dangling `\r`) confirmed, F2 (LF context lines) confirmed as pre-existing. |
| Claude fallback | SKIPPED | At least one external reviewer succeeded. |

## Verdict
PASS_WITH_NOTES

Both reviewers independently found the same real defect (CRLF dangling `\r`); the disagreement is only on blocking magnitude. Every confirmed issue is localized to `reconstruct` plus its test module, and the design's own Open Questions section already named the exact fix ("a targeted `\r` trim"). This is one bounded fix iteration, not a pivot or material divergence, so `PASS_WITH_NOTES` rather than Codex's `FAIL`.

## Must Fix Before PR
1. **CRLF no-final-newline leaves a dangling `\r`** (`src/resolve/mod.rs:644`). Traced: for a CRLF file whose original lacks a final newline and whose last hunk resolution also lacks one, the guard produces `out = "resolved\r\n"`, then the terminal `out.pop()` removes only `\n`, yielding `"resolved\r"`. Violates plan ST2 ("does not leave a dangling `\r`"). Fix: after the existing `out.pop()`, when `uses_crlf && out.ends_with('\r')`, pop the `\r` too.
2. **Strengthen the CRLF test** (`src/resolve/mod.rs:773`, `reconstruct_crlf_no_final_newline_preserved`). It asserts only `!ends_with("\r\n")` and `!ends_with('\n')`, both true for `"resolved\r"`, so it passes on wrong output (verified: test is green now). Add `assert!(!out.ends_with('\r'))` — ideally `assert_eq!(out, "resolved")`.
3. **Empty-replacement guard** (`src/resolve/mod.rs:619-625`). `Some("")` is reachable (classify `IdenticalSides` → `resolutions[i] = Some("")` at `resolve/mod.rs:315-316`). The guard appends a newline for empty text; because `out` already ends in a newline in mid-file, the only observable effect is a **leading blank line when an empty-resolved hunk is the first line of the file**. Narrower than Codex's framing, but still a behavior change this diff introduces. Fix: skip the guard when `text.is_empty()`, and add a regression test.

## Out of Scope / Deferred
- **Gemini F2 - LF endings on context lines in CRLF files** (`src/resolve/mod.rs:638-639`). `original.lines()` strips `\r`, so re-emitted context lines get LF. Real latent limitation, but pre-existing and untouched by this diff. Defer to a separate line-ending-fidelity task.
- **Codex LOW - trailing whitespace in `docs/discovery/clo-535.md:3-5`**. These are intentional markdown hard-break double-spaces, and `git diff --check` is not part of the declared pre-merge gate (`fmt/clippy/test`). Non-blocking; free to sweep in the same iteration if desired.

## False Positives / Tooling Artifacts
- **Codex "could not run `cargo clippy` / `cargo test`"** (read-only session, `.cargo-lock`). Tooling limitation, not a code defect. Closed here: `cargo clippy --all-targets -- -D warnings` → exit 0; `cargo test reconstruct_` → 6 passed, 0 failed. Note the passing suite includes the flawed CRLF test — which is exactly why fix #2 matters.

## Recommendation
**PROCEED_WITH_FIXES.** Do one bounded iteration in `src/resolve/mod.rs`: (1) strip the trailing `\r` in the terminal trim for CRLF files with no final newline; (2) tighten `reconstruct_crlf_no_final_newline_preserved` to assert `!ends_with('\r')` / `assert_eq!(out, "resolved")`; (3) skip the guard when `text.is_empty()` and add an empty-replacement regression test. Optionally sweep the docs trailing whitespace. All changes are confined to one private function and its test module, are anticipated by the design's Open Questions, and start from a green clippy/test baseline — so a single fix pass resolves everything without a pivot or user decision. Re-run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` before the PR transition.
