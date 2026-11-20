# Pre-PR Validation: clo-946

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Validated**: 2026-11-20
**Pipeline**: implement.md validation gate

---

## Verdict: FAIL

The supplied diff matches `d141fc4` against `main` at `9772049`. The branch catches the specified conflict fixtures, but a later `FAIL` can still be discarded. I reproduced that failure in the binding pre-PR workflow.

## Findings

- **HIGH — The dash matcher can hide a conflicting `FAIL` in the binding synthesis.** After an earlier `PASS`, both `FAIL --- PASS` and `FAIL - PASSor FAIL` returned `0` instead of `3`. P1 permits one or two ASCII dashes and requires a word boundary after the second token. The matcher accepts three dashes and reads `PASS` inside `PASSor`. In a full `lok` stub run, `FAIL --- PASS` produced a usable synthesis and the workflow exited `0`. [Guard](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:102) · [P1 specification](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:30)
- **MEDIUM — `prepr` applies the wrong case rule to the second token.** The parser uppercases the entire verdict line, so `FAIL / pass` and `FAIL - pass` become placeholders. Both returned `0` after an earlier `PASS`. Under AC2, lowercase `pass` is prose, so the `FAIL` stands and the conflict should return `3`. [Guard](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:98) · [AC2](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:46)

## Missing Items

- The committed suite needs explicit coverage of AC1’s three-line cutoff. Direct probes confirmed that a conflicting verdict on the third non-empty line returns `3` and the same verdict on the fourth returns `0`.
- GitHub has no PR for this branch and no hosted checks for `d141fc4`. The AC11 PR-body record and the Ubuntu and macOS CI results are pending.

The committed suite passed locally under `sh` and `dash` with macOS awk. All three specified workflow stub runs produced the expected named conflict and exit status. The corpus sweep found zero differences across 836 checks, and all three CLO-946 spec-review reports remained usable. CLO-537 returned `0` with a consistent `FAIL` verdict.

## Recommendations

Validate both tokens against the same phase-specific case rules, enforce the second token’s word boundary, and restrict dash matching to the four specified forms. Add regression fixtures for both findings and the lookahead cutoff, then rerun the binding workflow stub and record the evidence in the PR.