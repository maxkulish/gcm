# CLO-944 - AC-488-1 fails on main: retry notice matched as a failure

**Linear**: [CLO-944](https://linear.app/cloud-ai/issue/CLO-944)
**Type**: operational
**Date**: 2026-11-19

## Summary

`scripts/acceptance.sh` reported 294 pass and 1 fail on main (b724366). The fail was the second of the three AC-488-1 assertions, "no user-visible failure". The exit code (0) and the request count (3) passed. The cause was test drift, not a gcm defect. The fix changed only the assertion. After the fix, the suite reported 296 pass, 0 fail, 1 skip.

## Findings

1. **The failing assertion.** AC-488-1 ran `gcm --all --dry-run` against the `/retry429/v1` mock and failed when `grep -qi "rate limit"` matched the output. The output was:

   ```
   gcm: single-commit message: Groq (groq:openai/gpt-oss-120b), 1 file(s), ~31 B prompt
   gcm: [warn] Groq rate limited (HTTP 429); attempt 1 of 4, retrying in 0ns
   gcm: [warn] Groq rate limited (HTTP 429); attempt 2 of 4, retrying in 0ns

   Commit message (dry run - nothing staged or committed):
   ...
   ```

2. **The origin of the matched text.** CLO-798 (commit 96116ed, PR #60) promoted the retry notice in `retry_with` (`src/provider/http.rs:433`) from `debug_log!` to `warn_log!`, and changed the default log level from `Off` to `Warn`. Its spec requires this behaviour (AC-3 in `docs/specs/2026-09-16-clo-798-provider-call-visibility.md`): a retried request must say that it is retrying when no log variable is set. AC-488-1 predated CLO-798, and its case-insensitive "rate limit" grep matched the new progress notice.

3. **The real failure text is different.** When the retries run out, the `RateLimit` error prints `Groq rate limit reached (HTTP 429); wait a moment and retry, or use a different provider.` (`src/provider/identity.rs:37`). The retry notice says "rate limited", so the phrase "rate limit reached" separates the failure from the progress notice.

## Fix

In AC-488-1 (`scripts/acceptance.sh`):

- The negative assertion now matches `rate limit reached`, the terminal error text.
- A new positive assertion checks that `attempt 2 of` appears, so the case also records the CLO-798 behaviour that caused the drift.

gcm source code did not change. The retry policy, the request count and the exit code stayed the same.

## Verification

- `scripts/acceptance.sh` with a fresh release build: `PASS=296 FAIL=0 SKIP=1`. AC-488-1 passed all four assertions.
- Negative check: the same mock with `GCM_RETRY_MAX=1` exhausted the retries. gcm exited 1 and printed `rate limit reached`, which the new grep matched. The assertion therefore still detects a real rate-limit failure.

## Lessons

- A test that greps for a broad phrase in stderr breaks when a later task adds visible progress output with the same words. Match the exact failure text that the error `Display` produces.
- CLO-798 changed the default stderr output, and the AC-488-1 drift was first recorded during CLO-940. A change to default stderr output must include a full `scripts/acceptance.sh` run with a fresh release build.

## Follow-ups

None.
