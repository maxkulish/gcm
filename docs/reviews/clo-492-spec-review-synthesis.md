# Spec Review Synthesis: clo-492

**Synthesized**: 2026-06-21
**Pipeline**: lok spec-review

---

## Synthesis of Spec Reviews — CLO-492 Validation

**Sources:** Gemini (success), Ollama (success), Claude (skipped — at least one external reviewer succeeded). Both valid reviewers returned `APPROVE_WITH_SUGGESTIONS`. The two reviews are **largely complementary** — they converge on the spec's quality and contradict each other in only one place. Most of Ollama's value is a deep implementation-mechanics pass that Gemini did not attempt.

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | Problem statement is excellent — precise code refs (`src/plan.rs:243`, `src/main.rs:130`), clean mapping to FR-23/24/46/47 and ADR-001, explicit out-of-scope fencing (CLO-488/489/491). | Info (positive) |
| 2 | Acceptance criteria (AC-1..AC-10) are specific, measurable, and testable; concrete verification commands (`git status --porcelain=v1 -z`, `git write-tree`, request-count assertions). | Info (positive) |
| 3 | Decomposition is well-scoped: 5 sub-tasks each under ~2h; dependency order correct (1 & 2 independent leaves, 3→1, 4→2, 5→all). | Info (positive) |
| 4 | Constraints align with codebase + ADR-001 — pure/deterministic validator, sync/blocking (`std::thread::sleep`, no async), zero new deps, tolerant schema matching (only `groups[0]` message checked). | Info (positive) |
| 5 | **Duplicate-file handling is under-specified.** Gemini: AC-2 should name the duplicated file (symmetry with AC-1). Ollama: same-group duplicate (`["a.rs","a.rs"]`) is not in the test table. Same area, both flag it. | Low |
| 6 | **Curated-index warning's interaction with non-standard flags needs clarification.** Both independently flagged that the warning's trigger conditions under preview/automation flags are incompletely specified (specific flags differ — see Novel Insights). | Low–Medium |

---

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | Curated-index warning under `--yes` | Warning **still prints**; constraint only requires it be non-blocking (must not block `--yes`/non-interactive or add a prompt). | Questions whether it should be **suppressed entirely** under `--yes`, since the user opted into automation and may not want informational noise. | N/A (skipped) |

*Note: this is the only genuine conflict. Everything else is additive, not contradictory.*

---

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `validate_basic` → `validate` rename must update all existing test refs (e.g. `accepts_a_valid_plan`). | Ollama | Medium |
| 2 | `EmptyFirstGroup` → `EmptyGroup(0)` transition must be explicit: variant, `Display` impl (0-based internal / 1-based render), and all pattern matches. | Ollama | Medium |
| 3 | Missing test: **validation fails AND curated index exists** — both warnings should fire, in defined order. | Ollama | Medium |
| 4 | Document the unmerged→staged interaction in `main.rs`: unmerged conflicts return `true` for `is_staged()` but are intercepted upstream by the `is_unmerged()` abort guard, so warnings only fire on clean repos. | Gemini | Low |
| 5 | Future-proof: curated-index warning should bypass `--plan-only` (CLO-493) the way it bypasses `--dry-run`. | Gemini | Low |
| 6 | AC-7 should require a verifiable warning **substring** (e.g. must contain "curated index", "reset", "hunk-level staging is not preserved"). | Ollama | Low–Med |
| 7 | AC-7 should pin warning **timing**: after unmerged guard, before any plan-generation/staging, and before the fallback warning. | Ollama | Low |
| 8 | AC-5 "byte-identical" may be over-strict; clarify to "same tree SHA / same staged state" rather than binary compare of `.git/index`. | Ollama | Low |
| 9 | Note that CLO-488 retries already happen upstream (`retry_with` in `groq::send_chat`); validation failure returns immediately without re-requesting. Make explicit. | Ollama | Low |
| 10 | Cite ADR-001 Decision 9 to bind FR-46 (a "Should") as the required runtime warning. | Ollama | Low |
| 11 | Make `--reset` + curated-index interaction explicit (`--reset` doesn't touch staging → warning still fires). | Ollama | Low |
| 12 | Add an explicit documentation sub-task (README/`--help`, check `EGRESS_DISCLOSURE` wording consistency). | Ollama | Low |
| 13 | Add rename-in-change_set test (validator uses `path`, not `orig_path`); clarify `is_staged` reads index status `x` (first XY char). | Both (renames) / Ollama (clarify) | Low |
| 14 | Minor: invariant test for "plan has files but empty change_set" (guarded, but assert it); Test #17 should specify `--all` vs grouping path; `OmittedFile` fail-fast names only the first omission; `--dry-run` could optionally emit a "would reset" note; bash validator ref lives in `tmp/` (maybe not version-controlled). | Ollama | Info |

---

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** — both reviewers approved; no NEEDS_REVISION raised. The spec is implementable as-is; the items below are refinements, none blocking.

---

## Priority Actions

**P1 — Resolve before implementation (correctness mechanics):**
1. Make the `validate_basic`→`validate` rename plan explicit, including updating all existing test references. *(Novel #1)*
2. Make the `EmptyFirstGroup`→`EmptyGroup(0)` migration explicit: variant, `Display` (0-based store / 1-based render), all match arms. *(Novel #2)*
3. Add the **same-group duplicate** test case (`["a.rs","a.rs"]`) and update AC-2 text to name the duplicated file (mirror AC-1). *(Agreement #5)*
4. Add the **validation-fails + curated-index-exists** test, asserting both warnings and their order. *(Novel #3)*

**P2 — Decide and tighten (UX/spec clarity):**
5. **Human decision:** does the curated-index warning print or stay silent under `--yes`? Resolve the Gemini/Ollama split and record it in Constraints. *(Disagreement #1)*
6. Pin AC-7 warning **timing** (after unmerged guard, before staging, before fallback) and require a testable warning **substring**. *(Novel #6, #7)*
7. Clarify AC-5 to "same tree SHA / same staged state" rather than byte-identical. *(Novel #8)*

**P3 — Document / future-proof (nice to have):**
8. Note the unmerged→`is_staged()` interception in `main.rs`; future-proof the warning to bypass `--plan-only`; make `--reset` interaction explicit. *(Novel #4, #5, #11)*
9. Add explicit citations/notes: ADR-001 Decision 9 binds FR-46; CLO-488 retries are upstream; add the docs/`--help` sub-task and rename test. *(Novel #9, #10, #12, #13)*
