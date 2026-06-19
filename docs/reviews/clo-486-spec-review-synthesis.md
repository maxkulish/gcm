# Spec Review Synthesis: clo-486

**Synthesized**: 2026-06-19
**Pipeline**: lok spec-review

---

## Synthesis: CLO-486 Single-Commit Tracer Spec Review

Two external reviewers succeeded (Gemini, Ollama). Claude fallback was skipped per policy. Ollama's output was truncated before its verdict/blind-spots sections, so its verdict is inferred from finding severity.

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Non-TTY guard contradicts ADR-001 #10.** Spec treats non-TTY as out-of-scope ("proceed as interactive"), but ADR-001 Decision 10 requires erroring out. Will hang/crash on closed stdin in CI. Both flag the same contradiction. | High |
| 2 | **No HTTP error/timeout handling for Groq.** No explicit timeout constraint, no AC for 429/5xx, no eval tests for timeout/DNS failure. Should map cleanly to exit code 1. | High |
| 3 | **Index not restored on commit/signing failure.** Staging mutates the index right before commit; if `git commit -S` fails (pre-commit hook, GPG/signing key), the index stays dirty, breaking the transactional promise (AC-2). | High |
| 4 | **Subprocess stdio inheritance.** `$EDITOR` and `git commit -S` must inherit parent terminal stdin/stdout/stderr or interactive editors and pinentry/GPG passphrase prompts hang. (Gemini explicit; Ollama via $EDITOR fallback + launch-failure test.) | High |
| 5 | **git commit -S failure not covered.** No AC/test for signing-key-unavailable or pre-commit rejection (FR-58). | Medium |
| 6 | **Unborn branch / empty-tree diffing not a formal AC.** Fresh repo with no commits needs explicit handling; currently only an edge-case mention. | Medium |
| 7 | **Temp file cleanup not guaranteed across all paths** (success/abort/error/Ctrl-C). Should be a constraint, not an implementation note. | Medium |
| 8 | **acceptance.sh interactive prompt simulation underspecified.** No `--yes` flag or documented stdin-piping mechanism to drive Y/n/e non-interactively; Ollama adds it isn't explicitly scoped as a sub-task. | Medium |
| 9 | **Version stamping needs automation.** AC-1 demands a build-stamped version; Gemini wants `build.rs` promoted from optional→mandatory, Ollama wants Test #7 automated rather than manual. | Low |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | Overall verdict | APPROVE_WITH_SUGGESTIONS (explicit) | No verdict section emitted (truncated); findings are non-blocking suggestions | Skipped |

No material technical contradictions between the two reviewers — findings are complementary, not opposed.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Adaptive Groq payload by model ID.** Hardcoded `include_reasoning: false` may be ignored by reasoning models; send `reasoning_effort: "none"` for Qwen overrides via `GCM_GROQ_MODEL`. | Gemini | Medium |
| 2 | **`git write-tree` / `git read-tree <SHA>` as the concrete index-restore mechanism** (the "how" behind Agreement #3). | Gemini | Medium |
| 3 | **Document empty-tree magic SHA** `4b825dc642cb6eb9a0fa8e4ced7b1d4154961131` in git-layer guidelines. | Gemini | Low |
| 4 | **Sub-task 6 needs intermediate verification milestones** (verify raw diff gather before network send). | Gemini | Low |
| 5 | **Diff cap edge case:** 50-file / ~256 KB cap is ambiguous — what if files 1-50 are tiny but file 51 is 10 MB? Cap algorithm underspecified. | Ollama | Medium |
| 6 | **`--all` "no-op alias" contradicts PRD FR-6.** | Ollama | Medium |
| 7 | **Automated Conventional Commits format verification** missing from AC-1. | Ollama | Medium |
| 8 | **Empty/whitespace Groq response** handling not covered. | Ollama | Medium |
| 9 | **File misclassified as text despite NUL bytes** — missing test. | Ollama | Medium |
| 10 | **AC-4 "< ~3 s" is environment-dependent** — needs an absolute cap threshold. | Ollama | Low |
| 11 | **GroqError variant taxonomy** not specified (sub-task 4). | Ollama | Low |
| 12 | **Downstream unblocking criteria** for CLO-487/488/489/490 not stated; prompt-construction max-diff-size / file-ordering constraint absent. | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Gemini explicitly approved with suggestions; Ollama raised only non-blocking gaps (no NEEDS_REVISION). No reviewer blocked. The spec is solid for a greenfield tracer; the items below are pre-implementation hardening, not redesign.

## Priority Actions

Ordered by severity; agreement items first.

1. **Resolve the non-TTY contradiction** (Agreement #1) — align the spec UI section with ADR-001 #10: add a `stdin().is_terminal()` guard that exits 1 with an actionable message when no auto-confirm flag is present. Decide whether a `--yes` flag enters scope (also unblocks Action 7).
2. **Specify Groq HTTP failure handling** (Agreement #2) — add a default timeout (e.g. 30 s), an AC for 429/5xx/timeout/DNS → exit 1, and matching eval rows. Cover empty/whitespace responses (Novel #8).
3. **Guarantee index restoration on commit/signing failure** (Agreement #3 + Novel #2) — adopt `git write-tree` before staging and `git read-tree <SHA>` on any abort/failure; add AC + test for signing-key-missing and pre-commit rejection (Agreement #5, FR-58).
4. **Mandate subprocess stdio inheritance** (Agreement #4) — `Stdio::inherit()` for `$EDITOR` and `git commit -S`; document `$EDITOR`-missing fallback as a constraint.
5. **Formalize unborn-branch / empty-tree diffing** (Agreement #6 + Novel #3) — add AC and document the empty-tree SHA.
6. **Guarantee temp-file cleanup on all exit paths** (Agreement #7) — `tempfile` crate or a `Drop` wrapper; covers success/abort/error/Ctrl-C.
7. **Scope `acceptance.sh` interactive simulation** (Agreement #8) — define how Y/n/e is driven (stdin pipe or `--yes`).
8. **Make `build.rs` mandatory and automate the version test** (Agreement #9) to satisfy AC-1's build-stamp requirement.
9. **Tighten diff-cap semantics** (Novel #5) — define behavior when an over-cap file appears mid-list; add byte-cap precedence rule.
10. **Reconcile `--all` with PRD FR-6** (Novel #6) — confirm whether no-op alias is intended.
11. **Lower-priority polish:** adaptive Groq reasoning params (Novel #1), Conventional Commits format check (Novel #7), NUL-byte-misclassification test (Novel #9), absolute AC-4 threshold (Novel #10), GroqError taxonomy (Novel #11).
