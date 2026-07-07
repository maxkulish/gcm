# Review Synthesis: CLO-535 - Fix `gcm resolve` splice newline

**Synthesized**: 2026-07-07
**Pipeline**: lok design-review failed; manual Gemini fallback review
**Reviewers**: Gemini architect (manual fallback)

---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| lok design-review | FAILED | `ollama_review` step failed with unknown template variable `{{ steps.health_check.output }}` |
| pi run gemini-architect | UNAVAILABLE | `pi` CLI opened an interactive session TUI; not usable in non-interactive harness |
| Gemini architect (manual fallback) | OK | Produced structured review with verdict |

## Source

Single reviewer: Gemini architect manual fallback (`docs/reviews/clo-535-design-gemini.md`).

## Key Findings

| # | Finding | Severity |
|---|---------|----------|
| 1 | CRLF end-of-file no-final-newline corner may leave a dangling `\r` due to single-`\n` `out.pop()` | minor |
| 2 | Inline trailing-newline guard is correct; optional helper function for readability | nit |

## Verdict

APPROVE_WITH_SUGGESTIONS

## Priority Actions

1. **Implement phase**: add a unit test for the CRLF file with no final newline whose last hunk resolution also lacks a trailing newline. If it fails, update the terminal trim to strip a trailing `\r\n` pair when `uses_crlf` is true.
2. **Implement phase**: keep the guard inline unless another splice site needs the same logic.
