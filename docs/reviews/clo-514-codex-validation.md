# Pre-PR validation: clo-514

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-06-23
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

HIGH — AC7 no-regression is not met for legacy prefix tokens. The old scanner matched `ghp_`/`gho_`/etc. and `sk-` anywhere once they met the old minimum total length, but the new TOML rules require stricter live-shape lengths and `` boundaries: [rules.toml](/Users/mk/Code/gcm--feat-clo-514-secrets/src/privacy/rules.toml:41), [rules.toml](/Users/mk/Code/gcm--feat-clo-514-secrets/src/privacy/rules.toml:91). That means some values the old engine redacted, especially shorter bare `ghp_...` or `sk-...` fixtures, now pass unless they also appear in a keyword assignment. This conflicts with AC7 and the plan's explicit prefix migration requirement: [spec](/Users/mk/Code/gcm--feat-clo-514-secrets/specs/2026-06-23-clo-514-secret-rule-pack-entropy.md:55), [spec](/Users/mk/Code/gcm--feat-clo-514-secrets/specs/2026-06-23-clo-514-secret-rule-pack-entropy.md:184).

HIGH — The legacy keyword-assignment detector regresses for common declaration forms. The new generic assignment regex only accepts an identifier at the start of the line after optional diff/whitespace: [detect.rs](/Users/mk/Code/gcm--feat-clo-514-secrets/src/privacy/detect.rs:129). The keyword fast path then only applies to that captured identifier: [detect.rs](/Users/mk/Code/gcm--feat-clo-514-secrets/src/privacy/detect.rs:155). So `const password = "abcdefgh"` or `let token = "abcdabcd"` were caught by the old "find keyword anywhere on the line" logic, but now are not caught unless they meet the 16-char/high-entropy generic rule. Existing tests only cover `password=aaaaaaaa`/`token=abcdabcd` at line start: [detect.rs](/Users/mk/Code/gcm--feat-clo-514-secrets/src/privacy/detect.rs:388).

LOW — AC2's variant coverage is incomplete. The rule pack likely catches the alternations, but the table test only asserts one Slack variant and `sk_live_`; it does not assert `rk_live_` or the other required Slack family variants from `xox[bpsa]-`: [detect.rs](/Users/mk/Code/gcm--feat-clo-514-secrets/src/privacy/detect.rs:307), [rules.toml](/Users/mk/Code/gcm--feat-clo-514-secrets/src/privacy/rules.toml:73), [rules.toml](/Users/mk/Code/gcm--feat-clo-514-secrets/src/privacy/rules.toml:79).

## Missing Items

AC7 is not fully implemented: every old-engine detection does not still fire.

AC2 is only partially proven by tests for enumerated provider variants.

## Recommendations

Add regression tests that encode the old prefix detector contract exactly: old minimum lengths, bare tokens, embedded tokens, and assignment tokens for `AKIA`/`ASIA`, GitHub prefixes, `github_pat_`, and `sk-`.

Either broaden the TOML rules to preserve those old detections, or explicitly amend the spec to accept the narrower live-shape behavior.

Update the generic assignment matcher to handle declaration prefixes such as `const`, `let`, `var`, and multiple assignments per line, or add a second keyword-assignment compatibility pass matching the old scanner's behavior.

Expand AC2 tests for `rk_live_` and each required Slack variant.
