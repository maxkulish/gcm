# Pre-PR Validation: clo-946

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Validated**: 2026-11-20
**Pipeline**: implement.md validation gate

---

## Verdict: PASS_WITH_NOTES

The branch changes cleanly and accurately implement the specification in `/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md` across `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh`, the three workflow files, `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/tests/review-guard-test.sh`, and `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.github/workflows/ci.yml`. Portable POSIX `sh` / `awk` constraints, exit codes (`0`/`1`/`2`/`3`), caller contracts, and all committed test fixtures match the spec.

*(Note: Review performed by static inspection in read-only mode; test execution was not run directly in this session, though `/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/status/clo-946-workflow.yaml` records macOS `sh`/`dash` and Ubuntu `mawk`/`gawk` runs.)*

## Findings

1. **[LOW] Unconditional `toupper(s)` in `prepr` affects the second token in P1 alternation checks** (`/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:98-105`)
   - `verdict_value` calls `verdict_token(s)` first (line 83), so a line starting with lowercase `pass`, `pass_with_notes`, or `fail` is properly rejected before `toupper(s)` runs.
   - However, line 98 runs `if (phase == "prepr") s = toupper(s)` across the entire string before matching the second token in `t`. As a result, a line in `prepr` whose first token is valid uppercase followed by a lowercase non-token word (for example `FAIL - pass` or `FAIL / pass`) gets upper-cased to `FAIL - PASS` and classified as P1 (`"?"`) rather than `FAIL`. Unlikely in real reports, and matches how `single_verdict` previously behaved.

2. **[LOW] P3 regex requires horizontal whitespace at the end of closing markdown** (`/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:106`)
   - Line 106 uses `^[*_` \t]*[ \t](if|when|unless|otherwise)([^a-z0-9_]|$)`.
   - The spec defines closing markdown as any run of `*`, `_`, `` ` ``, and horizontal whitespace, and P3 as closing markdown containing at least one space or tab. A malformed line like `**PASS **if` (space inside closing markdown, none after `**`) would not match line 106, whereas standard markdown (`PASS if`, `**PASS** if`) matches as intended.

## Missing Items

None in the implementation or committed test suite:
- **AC1-AC9 & AC13**: Implemented in `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh`, `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/workflows/spec-review.toml:346`, `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/workflows/design-review.toml:387`, `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/workflows/pre-pr-validation.toml:426`, `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/tests/review-guard-test.sh`, and `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.github/workflows/ci.yml:59-60`.
- **AC10-AC12**: The committed test script covers the three AC19 reports and `clo-537-codex-validation.md`. The full corpus sweep (Evaluation #10), CLO-537 PR-body record (AC11 / Evaluation #11), AC12 spec-review report checks (Evaluation #12), and workflow stub runs (Evaluation #13) are PR-evidence / CI items (with results already logged in `/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/status/clo-946-workflow.yaml:73-79`).

## Recommendations

- **(Optional, LOW)** If strict `prepr` case rules are desired on the second P1 token as well, build `r` for `prepr` with `PASS_WITH_NOTES|PASS|FAIL` uppercase-only and `[Aa][Pp][Pp][Rr][Oo][Vv][Ee]...` / `[Rr][Ee][Ww][Oo][Rr][Kk]` case-insensitive patterns instead of `s = toupper(s)` in `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:98`.
- **(PR checklist)** Ensure the PR description includes the Evaluation #10-#13 outputs recorded in `/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/status/clo-946-workflow.yaml` and confirm the new `Review guard tests` CI step passes on both `ubuntu-latest` and `macos-latest` (Evaluation #14).
