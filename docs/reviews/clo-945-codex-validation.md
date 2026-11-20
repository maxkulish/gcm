# Pre-PR Validation: clo-945

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Validated**: 2026-11-20
**Pipeline**: implement.md validation gate

---

## Verdict: FAIL

Reviewed `7545d1a` against `main` and the CLO-945 spec. The supplied diff matches HEAD. All three workflows passed `lok workflow validate`, and all three AC19 reference reports passed the shared checker. The saved AC17 outputs contain usable reviews from both models that cite the spec. Live provider evidence came from those saved runs.

## Findings

1. **HIGH — Conflicting verdicts pass the binding synthesis check.** I reproduced a pre-PR synthesis with `PASS` under `## Verdict` and `FAIL` under `## Final Verdict` that the checker accepted. It stops at the first qualifying heading. It also accepted a placeholder sentence starting with `PASS` and a synthesis containing `NO_REVIEWS_AVAILABLE` inside a code fence. The writer can publish these outputs as valid syntheses. [Verdict checker](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/lib/review-guard.sh:89) · [Pre-PR writer](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:421)

2. **HIGH — A directory at the expected file path can make publication appear successful.** `mv -f` moves the staged file *into* an existing directory and returns zero. When the pre-PR synthesis target is a directory, the subsequent `sed` read fails, but the writer still exits zero. This can bypass the required nonzero exit for zero usable reviews or failed synthesis. The same move pattern can start reviewers with a directory as their diff input and affect report publication in the other entry points. [Pre-PR publication](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:434) · [Diff preparation](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/pre-pr-validation.toml:43) · [Inline publication](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/implement.md:129)

3. **MEDIUM — Slug validation leaves intermediate writes exposed.** The health check rejects an invalid slug but exits successfully. Later steps rebuild the run path from the same argument and write status files and fallback artifacts before the writer validates it again. A slug with parent-directory components can direct those writes outside the review-run directory. All three lok workflows share this path. [Health check](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml:26) · [Status writes](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml:115) · [Fallback writes](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/workflows/spec-review.toml:138)

4. **MEDIUM — The inline gate honors the test-only usability bypass.** With `REVIEW_GUARD_SKIP_USABLE=1`, I confirmed that the guard returned prose with no verdict as a successful result. The lok workflows recheck the report in `review_status`. The inline gate has no equivalent check and marks any output without the failure prefix usable, so it can set `codex_validated: true` for an unusable report. [Guard bypass](/Users/mk/Code/gcm--fix-clo-945-review-gates/.lok/lib/review-guard.sh:188) · [Inline classification](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/implement.md:117)

5. **MEDIUM — The inline wrapper masks failure in its shell exit status.** It prints the child’s exit code, then returns the status of `echo`. A caller checking the block’s exit status sees success after an input, setup, or publication failure. The documented pause-only branch depends on reading the printed text. [Inline wrapper and failure handling](/Users/mk/Code/gcm--fix-clo-945-review-gates/.claude/commands/task/phases/implement.md:141)

## Missing Items

- **AC18 and AC25** need an unambiguous verdict across the report and rejection of the reserved synthesis sentinel throughout the output.
- **AC9, AC14 and AC23** need exact-path publication and a failing gate when the required diff or report cannot be written at its expected path.
- **AC14 and AC16** need an unconditional inline usability check and an outer exit status that preserves gate failures.

## Recommendations

- Scan the entire report for verdicts, reject conflicting headings and placeholder verdicts, and check for the reserved sentinel before skipping fenced content. Keep the AC19 fixtures in the regression cases.
- Validate the slug before any run-path access. Use an exact-target rename for diffs and reports, and derive the pre-PR exit from the writer’s verdict decision after successful publication.
- Keep the bypass in the test harness, propagate the inline script’s exit status after cleanup, and rerun the stub matrix with directory destinations, invalid slugs, malformed verdicts, and forced inline publication failures.