# Pre-PR Validation: clo-946

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Validated**: 2026-11-20
**Pipeline**: implement.md validation gate

---

## Verdict: PASS_WITH_NOTES

The current implementation on disk in `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh`, `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/tests/review-guard-test.sh`, the three `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/workflows/*.toml` files, and `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.github/workflows/ci.yml` satisfies AC1–AC9 and AC13 of `/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/specs/2026-11-20-clo-946-verdict-check.md`, and resolves the `d141fc4` validation findings (`---` dash matching, `PASSor` boundary, `prepr` `toupper` on the second token, and explicit 3-line lookahead test coverage).

*(Note: This review was performed by static inspection in read-only mode; `.lok/tests/review-guard-test.sh` was not executed in this session.)*

## Findings

1. **[MEDIUM] Snapshot diff is stale relative to the working tree** (`/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/local/review-input/clo-946-branch.diff:70-100`)
   - `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/local/review-input/clo-946-branch.diff` still reflects the earlier `d141fc4` state (with `if (phase == "prepr") s = toupper(s)` and `-+` dash alternation), whereas the actual files on disk (`/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:82-107` and `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/tests/review-guard-test.sh:76-83,146-149`) already include the fixes for the prior Codex findings.
   - Ensure validation evidence and the PR diff are captured from the current HEAD/working tree rather than the stale `d141fc4` snapshot.

2. **[LOW] P3 regex requires whitespace immediately before the condition word** (`/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:105`)
   - `match(tolower(t), "^[*_` \t]*[ \t](if|when|unless|otherwise)([^a-z0-9_]|$)")` handles normal markdown (`PASS if ...`, `**PASS** otherwise`), but would not match malformed closing markdown where whitespace precedes trailing formatting characters (for example `**PASS **if ...`), even though the spec defines P3 as closing markdown containing at least one space or tab.

3. **[LOW] Trailing `_` in closing markdown class can match an underscored suffix in P1** (`/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:103-104`)
   - Because `[*_` \t]*` / `[*_` \t]+` includes `_`, an unlikely input such as `FAIL - PASS_or FAIL` or `**PASS**_or FAIL` can match `_` as closing markdown before `or ` rather than treating `_` as part of an identifier boundary.

## Missing Items

- **PR & CI verification (AC10–AC12 / Evaluation #10–#14)**: Opening the PR, confirming `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.github/workflows/ci.yml` (`Review guard tests`) passes on both `ubuntu-latest` and `macos-latest`, and recording the post-fix corpus sweep (AC10), CLO-537 check (AC11), AC12 spec-review checks, and workflow stub runs (Evaluation #13) in the PR body.

## Recommendations

1. Re-run `sh /Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/tests/review-guard-test.sh` and the Evaluation #10–#13 checks against the updated `.lok/lib/review-guard.sh`, refresh `/Users/mk/Code/gcm--fix-clo-946-verdict-check/docs/status/clo-946-workflow.yaml` if needed, and record the results in the PR body.
2. *(Optional, LOW)* If you want exact closing-markdown compliance for P3 in `/Users/mk/Code/gcm--fix-clo-946-verdict-check/.lok/lib/review-guard.sh:105`, match `^[*_` \t]*[ \t][*_` \t]*(if|when|unless|otherwise)([^a-z0-9_]|$)`.
