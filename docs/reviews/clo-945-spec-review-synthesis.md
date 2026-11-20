# Spec Review Synthesis: clo-945

**Synthesized**: 2026-11-19
**Reviewers**: Gemini (google-vertex/gemini-4-hydrogen#high), Codex (gpt-6.2-sol, effort high)
**Pipeline**: manual run of the fixed reviewer commands

---

## Reviewer Status

Both external reviewers produced a real review of the draft spec. Both exited 0. The Gemini review has 18363 bytes and the Codex review has 10261 bytes, and codex reported `model: gpt-6.2-sol` and `reasoning effort: high`. The Claude fallback was not needed.

Both verdicts were **NEEDS_REVISION**.

## Agreement (High Confidence)

| # | Finding | Section | Severity |
|---|---------|---------|----------|
| 1 | `design-review.toml` references the skipped `claude_fallback` without `is defined` guards (lines 227-228, 305-309), so the workflow fails once a reviewer succeeds. The Must "keep synthesis as is" blocked the fix. | 3, 4 | Critical |
| 2 | `'REVIEW_FAILED' in output` is a substring match, so a real review that quotes `REVIEW_FAILED` (any review of this spec) counts as a failure. AC6 help-text matches need the same anchoring. | 2, 7 | High |
| 3 | Evaluation #1 rejects the valid `lok run --dir`, and Evaluation #3 passes when one of four strings matches. | 5 | High |
| 4 | The scope said 8 files, but Section 4 listed 10. The two `.claude/templates/` files that name Ollama were missing. | 1, 4 | Medium |
| 5 | The inline gate used literal models, so `CODEX_MODEL` and `GEMINI_MODEL` did not apply there. | 2 | Medium |
| 6 | The stub tests could miss the review branch (the health probe matches `ok` loosely), and the edge cases had no test rows. | 5 | Medium |
| 7 | The `review_ollama` rename breaks the reader side for workflows started under Pi or before this change. | 6 | Medium |

## Findings (Gemini Only)

| # | Finding | Severity |
|---|---------|----------|
| 1 | `.lok/lok.toml` keeps `model = "glm-5:cloud"` and `google/` on both `model` and `args`. | High |
| 2 | Health checks ignore the opencode exit status and grep `ok` as a substring. | High |
| 3 | Multi-line stderr breaks the one-line `REVIEW_FAILED` contract. | Medium |
| 4 | lok shell steps need `set -e`-safe exit capture and POSIX `.` sourcing. | Medium |
| 5 | `pre-pr-validation.toml:9` points to the `.pi/orchestrator` implement file. | Low |

## Findings (Codex Only)

| # | Finding | Severity |
|---|---------|----------|
| 1 | A validator rejection keeps the raw output, and the writers can publish it under a normal header. | Critical |
| 2 | Pre-PR synthesis can return PASS with zero usable reviews, its final check accepts stale files, and the inline gate records `codex_validated: true` unconditionally. | Critical |
| 3 | A 300 s child timeout inside a 5 m lok step leaves the guard no time; the bare `wait` loses each exit status. | High |
| 4 | `--auto` approves every permission that is not denied; the Gemini reviewer needs read-only scope. | High |
| 5 | AC11 passed when both live reviewers failed, so it could not prove the fix. | High |
| 6 | lok appends `--model` itself; repeating it in `args` gives two model flags. | Medium |

## Disagreement

None. Codex suggested keeping Pi compatibility or updating Pi; Gemini called the rename safe to defer. The spec does both halves without touching `.pi/`: the Claude phase files read a legacy `review_ollama` key, and the Pi update stays a follow-up.

## Verdict

**NEEDS_REVISION**, resolved by the revision applied to the spec on 2026-11-19 (see the workflow YAML `applied_suggestions`). No suggestion contradicted a maintainer decision.
