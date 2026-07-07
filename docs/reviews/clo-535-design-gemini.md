# Design Review: CLO-535 - Fix `gcm resolve` splice newline

**Reviewer**: Gemini architect (manual fallback)
**Reviewed**: 2026-07-07
**Pipeline**: lok design-review failed (template variable error); manual fallback review

---

## Context

- Branch: `fix/clo-535-new-line`
- Design: `docs/designs/clo-535-fix-resolve-splice-newline.md`
- Discovery: `docs/discovery/clo-535.md`
- PRD: `docs/prds/clo-535-fix-resolve-splice-newline.md`

## Findings

### F1 [minor] CRLF end-of-file corner is correctly flagged as an open question
**Where:** design doc § Open questions
**What:** The design leaves unresolved whether the terminal `out.pop()` that trims a single `\n` could leave a dangling `\r` when a CRLF file has no final newline and the last hunk resolution also lacks one.
**Why it matters:** The PRD acceptance criterion says "Files with no final newline are unchanged in that respect (existing behavior preserved)." For CRLF files the current `pop()` may not preserve byte-identical behavior.
**Suggested fix:** Add a dedicated unit test for this exact corner in the implement phase. If it fails, update the trim to strip a trailing `\r\n` pair for CRLF files rather than a single `\n`.

### F2 [nit] Helper function would make the guard more readable
**Where:** design doc § Public API surface, `src/resolve/mod.rs`
**What:** The inline `if uses_crlf { if !out.ends_with("\r\n") { ... } } else if !out.ends_with('\n') { ... }` guard is correct but slightly noisy when repeated mentally across hunk iterations.
**Why it matters:** A small private helper (`ensure_trailing_newline`) would reduce duplication if future splice paths need the same guard, but for a single branch the inline form is acceptable.
**Suggested fix:** Optional; keep inline unless the implement phase reveals another splice site that needs the same logic.

## Strengths

- Change is tightly scoped to one private function; no public API churn.
- Line-ending handling reuses the existing `uses_crlf` detection and composes correctly with the LF→CRLF normalization path.
- Test matrix covers LF/CRLF × resolution-trailing-newline × original-trailing-newline, including the regression case that fails before the fix.
- Assumptions are explicit and have concrete verification paths.
- Non-goals correctly exclude prompt/schema changes and unrelated resolve behaviors.

## Verdict

APPROVE_WITH_SUGGESTIONS

The design is sound and ready for implementation. The only follow-up is to verify the CRLF no-final-newline corner with a unit test and adjust the trim if needed; this is already captured as an open question in the design doc.
