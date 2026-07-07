# Lessons: CLO-535 — Fix `gcm resolve` splice when resolution lacks a trailing newline

Source: `docs/status/clo-535-workflow.yaml`, `docs/reviews/clo-535-validation-synthesis.md`
Date: 2026-07-07

---

## L1 - Defensive normalization at the splice point beats relying on provider output conventions

**Source incident**: CLO-535 validation gate (Codex + Gemini review). `reconstruct` in `src/resolve/mod.rs` appended provider resolution text verbatim and assumed it ended in `\n`. When a provider omitted the trailing newline, the next context line fused onto the last resolved line.

**Rule**: When splicing externally supplied text into a file with strict line-ending conventions, explicitly ensure the splice boundary ends with exactly one line ending in the file's dominant line-ending style. Do not trust the external text to provide the delimiter.

**How to apply**: In `reconstruct`, after pushing the resolution text (including any CRLF normalization), append one line ending if missing. Use `\r\n` for CRLF files and `\n` otherwise. Skip the guard for empty replacement text so "delete the hunk" does not become "insert a blank line".

---

## L2 - A green test suite can pass on wrong output; assertions must target the actual defect

**Source incident**: CLO-535 validation gate. `reconstruct_crlf_no_final_newline_preserved` asserted only `!out.ends_with("\r\n")` and `!out.ends_with('\n')`, both true for the malformed `"resolved\r"`. The test stayed green while the CRLF no-final-newline case produced a dangling carriage return.

**Rule**: When a defect produces a specific malformed tail, assert the exact absence of that tail, not just the absence of a correct tail. For line-ending bugs, check the dangling partial (e.g. `!ends_with('\r')`) and, where possible, assert the whole expected string.

**How to apply**: In CRLF/no-final-newline tests, add `assert!(!out.ends_with('\r'))` and `assert_eq!(out, "resolved")` so a partial trim cannot slip through.

---

## L3 - Existing terminal trim may not compose with new line-ending guards

**Source incident**: CLO-535 validation gate. The terminal trim in `reconstruct` removed a single `\n` to preserve files without a final newline. After adding a CRLF guard, the pop left a dangling `\r` because the trim was not CRLF-aware.

**Rule**: When adding a line-ending guard, re-examine the final trim. A single-character pop that is correct for LF may be incorrect for CRLF.

**How to apply**: After the existing `out.pop()` for no-final-newline files, add a second pop: if `uses_crlf && out.ends_with('\r')`, remove the carriage return too. Keep the single-pop path for LF files.
