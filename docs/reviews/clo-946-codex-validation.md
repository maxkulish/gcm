# Pre-PR Validation: clo-946

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Validated**: 2026-11-20
**Pipeline**: implement.md validation gate

---

## Verdict: FAIL

Reviewed the supplied diff at `1ca7d7e` and the subsequent branch commit `12ac629` against `main` at `9772049`. Both revisions can still accept a binding pre-PR synthesis that contains conflicting verdicts.

## Findings

- **HIGH — The dash matcher can hide a later `FAIL`.** After an earlier `PASS`, both `FAIL - PASS_` and `FAIL - PASS --- details` return `0` in both pre-PR phases. The first slips past P1’s second-token boundary. The second lets the regex match a prefix of an unsupported three-dash separator. Both should return `3`. A stubbed pre-PR workflow on the supplied revision published the first synthesis as usable and exited `0`. [Guard](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:106) · [P1 specification](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:30)
- **MEDIUM — The latest commit rejects reports with one valid verdict.** A report whose sole verdict is `FAIL - fail` now returns `3` in both pre-PR phases and gets reported as conflicting. Under AC2, lowercase `fail` is prose. Under AC4, the sole `FAIL` must keep the report usable so its findings reach the gate. The new fixtures also expect `3` for standalone `PASS --- FAIL`, which the approved grammar reads as `PASS`. [Guard](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:108) · [Tests](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/tests/review-guard-test.sh:84) · [Acceptance criteria](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:46)
- **MEDIUM — Markdown can turn a conditional placeholder into a verdict.** `PASS **if** tests pass` returns `0` as the sole verdict and creates a false conflict alongside a valid `FAIL`. P3 allows whitespace anywhere in the run of Markdown markers before `if`. The matcher requires whitespace immediately before `if`. [Guard](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:107) · [P3 specification](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:32)

## Missing Items

- **AC13** — The committed suite checks `FAIL - PASS criteria were not met` only in `prepr`. AC6 also calls for that case in `synthesis-prepr`. [Test fixture](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/tests/review-guard-test.sh:62)
- **AC11 and hosted CI** — CLO-537 returned `0` with a consistent `FAIL`. GitHub showed no PR for the branch, so the PR-body evidence and Ubuntu/macOS CI results are pending. [AC11](/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md:55) · [CI matrix](/Users/mk/Code/gcm--fix-clo-946-verdict-check/.github/workflows/ci.yml:17)

## Recommendations

Enforce the second-token boundary and match the full dash separator. Accept P3 whitespace anywhere in the allowed Markdown run. Reserve exit `3` for distinct verdicts from qualifying sections, then add regression fixtures for the cases above.

The committed suite passed under `dash` on macOS. Each revision matched `main` on all 828 stored-file checks. All three workflow stubs produced the expected status and named failure for ordinary conflicts on the supplied revision.