# Pre-PR Validation: clo-947

**Reviewer**: Codex (gpt-6.2-sol, effort high)
**Validated**: 2026-11-20
**Pipeline**: implement.md validation gate

---

## Verdict: PASS_WITH_NOTES

Reviewed `1f7cc10` against `main` and the CLO-947 spec. All 22 guard checks passed under `sh` and `dash` on macOS. Stubbed runs passed the 18 lok report-directory cases, both pre-PR diff failure cases, and all three inline failure cases. All four gates matched `main` when creating and replacing reports.

## Findings

- **LOW — An unwritable diagnostic record can hide the publication failure.** When `publish-failed` is a directory, the writer’s cleanup emits an error and the helper falls back to printing the target path. With a slug containing `429`, I reproduced lok displaying `rate limited` while correctly blocking the gate. [Review guard](/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/lib/review-guard.sh:255) · [Writer cleanup](/Users/mk/Code/gcm--fix-clo-947-report-publish/.lok/workflows/spec-review.toml:279)
- **LOW — The wiring test misses a broken inline publish chain.** It checks for `|| exit 1` anywhere on the line. In a temporary checkout, replacing `&&` between the two publishes with `;` still passed every guard check. That change would let a successful Gemini publish mask a failed Codex publish. The current branch uses the correct chain. [Wiring test](/Users/mk/Code/gcm--fix-clo-947-report-publish/scripts/test-review-guard.sh:160) · [Inline gate](/Users/mk/Code/gcm--fix-clo-947-report-publish/.claude/commands/task/phases/implement.md:133)

## Missing Items

- **AC3a / AC5:** A lok-visible diagnostic for failure to clear or write `publish-failed`.
- **AC9:** A test assertion that enforces the inline short-circuit required by AC4. The Ubuntu and macOS CI steps are wired, and hosted results remain unverified.

## Recommendations

1. Handle record cleanup and write failures with a distinct, fixed stderr line that says the diagnostic could not be saved. Keep raw filesystem errors out of lok’s stderr summary and document the fallback.
2. Assert the full `review_publish … && review_publish … || exit 1` chain in the wiring test.