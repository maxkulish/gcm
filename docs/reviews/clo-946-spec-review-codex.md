# Spec Review: clo-946

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Reviewed**: 2026-11-20
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The spec identifies the correct failure point and matches the core Linear task. At `469b357`, read-only probes confirmed that the current guard accepts both a `PASS` followed by a `FAIL` and a report whose only verdict is `PASS if no finding is HIGH or above`. The three-line lookahead is an inherited limit that the spec should state alongside its whole-report guarantee. [Current verdict scan](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:103) · [Spec AC1](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:36)

Preserving a clear `FAIL` report keeps its findings available. In Pi, the binding synthesis verdict controls PR progression. The inline Claude gate presents usable reports to the owner, who can choose to proceed. The spec should distinguish those outcomes when interpreting “the gate does not pass.” [Pi phase decision](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.pi/orchestrator/phases/implement.md:277) · [Inline decision handling](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.claude/commands/task/phases/implement.md:177)

## 2. Acceptance Criteria Review

**Strong.** AC3–AC8 give measurable outcomes for missing, repeated, and conflicting verdicts, including exact failure messages. AC11 names the CLO-537 artifact and requires PR evidence. The current guard returned `0` for that report in `prepr`. It contains one template alternation and two identical `FAIL` verdicts. [CLO-537 template](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/reviews/clo-537-codex-validation.md:30) · [First actual verdict](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/reviews/clo-537-codex-validation.md:10418)

**Gaps.** AC9 promises the same heading-count result, but accepting repeated values within a section can move `vline`. With `## Verdict: PASS`, a body line of `PASS`, and `### Final Verdict: PASS`, the current guard selects the H3 and counts the H2, yielding `0`. Under AC9’s proposed selection, it excludes the H2 and yields `1`. An in-memory probe confirmed that change. AC12’s glob also matches the synthesis file, which uses `synthesis-review` and requires `Priority Actions`. [Heading selection and count](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:118) · [AC9–AC12](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:44)

## 3. Constraints Check

**Aligned.** POSIX shell and portable awk fit the shared helper and the existing macOS/Linux CI matrix. The constraints preserve the scope of CLO-949. [CI matrix](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.github/workflows/ci.yml:11) · [Spec constraints](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:52)

**Concerns.** Reusing `single_verdict` unchanged conflicts with the requirement to accept verdicts followed by explanatory prose. Its alternation regex matches the start of the suffix and allows arbitrary text after the second token. AC2 should also spell out that case folding applies to the legacy `prepr` tokens, while `PASS`, `PASS_WITH_NOTES`, and `FAIL` remain uppercase-only. [Token recognition and alternation regex](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:52)

## 4. Decomposition Quality

**Well-scoped.** The parser, caller wiring, tests, and evidence are sensible units, plausibly within two hours each once the grammar is settled. The dependency order is accurate.

**Issues.** Assign AC12 and the three workflow checks in Evaluation #13 to a subtask. AC13 puts the full AC10 corpus check in the committed script, while subtask 3 names selected fixtures and Evaluation #10 assigns the full sweep to a scratch script. Specify which checks run in CI and pin the comparison revision for the PR evidence. [Decomposition](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:76) · [Evaluation](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:97)

## 5. Evaluation Coverage

**Covered.** The shell fixtures suit the parser contract, the archived reports give a compatibility baseline, and the existing CI matrix can exercise both awk environments.

**Gaps.** The heading-only conflict fixture in #1 is 68 characters and would hit `review_guard`’s 100-character floor in #9. Pad the wrapper fixtures so the assertions reach verdict parsing. The red-first check should cover the conflict and P3 cases. The P1 and P2 fixtures already return `1` on the current guard. Finally, #13 should exercise all three actual workflow writers and assert their outer outcomes. A conflicting binding synthesis must produce the named failure and a nonzero pre-PR run. Spec and design review must still publish the usable individual reports. [Guard checks](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:173) · [Pre-PR publication and exit](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/workflows/pre-pr-validation.toml:430) · [CLO-945 workflow contract](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-19-clo-945-review-gates.md:77)

## 6. Codebase Alignment

**Violations.** None at the Rust boundary. This path uses shell exit statuses and report prefixes. The Rust CLI’s `Provider`, `ProviderError`, and `GcmError` contracts sit outside it, so the `Backend`, `BackendErrorKind`, and `anyhow` criteria are inapplicable. [Provider trait](/Users/mk/Code/gcm--fix-clo-946-verdict-check/src/provider/facade.rs:43) · [CLI error contract](/Users/mk/Code/gcm--fix-clo-946-verdict-check/src/error.rs:6)

**Alignment.** Exit `3` fits the helper contract. The callers should capture the original status in a conditional failure branch and preserve the existing report prefixes, stderr excerpts, and workflow-specific outcomes. The review verified the existing guard across 206 stored reports in four phases. Post-change parity and workflow outcomes remain implementation checks.

## 7. Blind Spots

**High — A later genuine `FAIL` can still be discarded.** A probe confirmed that `single_verdict` classifies `FAIL - PASS criteria were not met` as an alternation. Under the proposed reuse, `## Final Verdict: FAIL - PASS criteria were not met` would be ignored after a valid `PASS`, leaving the binding synthesis usable. [Alternation matcher](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:75) · [Spec placeholder rule](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:24)

**Medium — A copied prompt fragment can still supply a verdict.** The pre-PR synthesis prompt contains `- PASS - no Must Fix items` under `## Verdict`. A fragment that echoes that line and the required `Must Fix Before PR` heading passed the current check and falls outside P1–P3. [Synthesis prompt](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/workflows/pre-pr-validation.toml:312)

## 8. Verdict

NEEDS_REVISION

## 9. Actionable Feedback

1. Define alternation by the complete token list and explicitly handle the existing synthesis prompt’s legend. Add fixtures for both a later `FAIL` with explanatory prose and a prompt-echo-only synthesis.
2. State the verdict lookahead boundary, settle the mixed-heading `vline` behavior, and give reviewer and synthesis artifacts phase-specific AC12 checks.
3. Reconcile CI and PR evidence requirements. Exercise the actual workflow writers with stubs and assert the published first line and outer exit status.
