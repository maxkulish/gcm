# Spec Review Synthesis: clo-487

**Synthesized**: 2026-06-20
**Pipeline**: lok spec-review (Gemini 3.5 Flash + Codex/Ollama glm-5:cloud). The automated synthesis + write steps failed (`nul byte found in provided data` - a NUL byte in a reviewer's output broke the Claude synthesis subprocess, the very class of bug this spec hardens against). This synthesis was produced manually by the orchestrator from the salvaged reviews.

---

## Consolidated Verdict
**APPROVE_WITH_SUGGESTIONS** (Gemini: APPROVE_WITH_SUGGESTIONS; Ollama: APPROVE_WITH_SUGGESTIONS; no NEEDS_REVISION → consolidated APPROVE_WITH_SUGGESTIONS). No redesign required.

## Agreement (High Confidence)
| # | Finding | Severity | Source |
|---|---------|----------|--------|
| 1 | **Rename staging bug**: stage only-new-path leaves the old path's deletion unstaged → split rename. Stage both `<new>` and `<orig>` for `R`/`C`. | Critical | Gemini (Ollama implicitly via delete-handling) |
| 2 | **Clear-index on unborn branch**: `read-tree HEAD` fails with no HEAD → empty-index fallback (`read-tree --empty` / empty-tree SHA). Pick `read-tree HEAD` (has HEAD) + `--empty` (unborn). | High | Gemini + Ollama |
| 3 | **Embed the concrete JSON Schema** for `Plan` (required props, `additionalProperties:false`, nullable `commit_message`) - critical for `strict:true`. | Medium | Ollama (Gemini implicit) |
| 4 | **Define the grouping system prompt** (inputs: file list + porcelain status + diff stat + per-file diffs; rules ported from bash 305-322). | Medium | Ollama |
| 5 | **Tracked-diff per-file truncation**: split `git diff` on `diff --git ` boundaries, cap each section, `[diff omitted: N bytes]`. | Medium | Gemini + Ollama |
| 6 | **Resolve clear-index approach** explicitly (`read-tree`), consistent with `restore_index`. | Medium | Ollama (Gemini) |
| 7 | **Clarify `MAX_TOTAL_BYTES` vs per-file caps**: per-file during assembly; total = final safeguard. | Low | Ollama |

## Disagreement (Needs Human Decision)
| # | Topic | Position |
|---|-------|----------|
| 1 | **Rename NUL field order** in `git status --porcelain=v1 -z` | Gemini contradicted *itself* (actionable #4: new-path-first/orig-next; doc-note #8: orig-first/new-next). Ollama: order unspecified, document it. **Resolution: do NOT hardcode; the empirical `git mv` test in a temp repo is the single source of truth (spec constraint retained and strengthened).** No human decision needed - the test settles it. |

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | Interactive `e` edits only group 1's message this run; later groups re-analyzed next run - make explicit. | Gemini | Low |
| 2 | `--all --dry-run` should print the single-commit message and exit (no grouping/staging/commit). | Ollama | Low |
| 3 | Add eval rows: single-group plan, `groups: []`, `commit_message:null` in group 1, delete-only group, `strict:true` in payload, unborn-branch grouping. | Both | Low-Med |
| 4 | Fallback error message format - give concrete example text. | Ollama | Low |
| 5 | Prefer a new `GroupingContext` struct over extending `GatheredDiff`. | Ollama | Low |
| 6 | Concurrency: change set captured once at start; stale plan paths → fallback. | Ollama | Low |
| 7 | HTTP-timeout-specific test (hard to script; consider a unit test; carry existing 30s timeout). | Ollama | Low |

## Priority Actions (applied to the spec)
1. (Critical) Rename staging: stage `<new>` + `<orig>` for `R`/`C` - Constraints + sub-task 2 + AC-4 + eval row.
2. (High) Clear-index: `read-tree HEAD`, `read-tree --empty` on unborn - Constraints + sub-task 5.
3. (Med) Embed JSON Schema + grouping system prompt - new "Provider contract" subsection + sub-tasks 1 & 4.
4. (Med) Tracked-diff section-split truncation - sub-task 3 + Constraints.
5. (Low-Med) Add eval rows (single-group, empty-array, null-message, delete-only, strict-payload, unborn-branch).
6. (Low) Clarify `--all`/`--all --dry-run`, interactive-edit scope, fallback message text, `MAX_TOTAL_BYTES` relationship, `GroupingContext` struct, concurrency note.

All items are additive/refinement and none contradict ADR-001 → auto-applied; zero user-conflict prompts. HTTP-timeout unit test noted as a lower-priority follow-up (carry existing 30s timeout; AC-7 covers the fallback path).
