## Verdict: PASS_WITH_NOTES

Reviewed `main...0700eae` against the design, implementation plan, and first review. The approved D5 change is implemented correctly: all message pre-flights use `ceil(bytes / 4)`; grouping retains its existing estimator. Envelope and completion reserves remain unchanged.

## Findings

1. **LOW — Paths-only rendering can exceed the configured byte ceiling.**  
   The final rung checks tokens but bypasses `diff_budget.total_bytes` in [src/diff.rs:355](/Users/mk/Code/gcm--fix-clo-797-grouping-prompt/src/diff.rs:355). With `GCM_DIFF_TOTAL_BYTES=1`, I captured a grouping request containing the 64-byte omission marker. This violates ST4’s explicit body ceiling, although the token budget remains satisfied.

2. **HIGH — Emoji undercount remains, explicitly deferred to CLO-921.**  
   This is non-blocking under the owner’s decision. Besides grouping degradation and pre-flight, the same estimator controls the path-echo completion reserve and Anthropic output-cap rejection in [src/budget.rs:409](/Users/mk/Code/gcm--fix-clo-797-grouping-prompt/src/budget.rs:409). Emoji-heavy inventories can therefore under-reserve completion space or evade that rejection. The generous message estimator is independent.

First-review disposition:

| Item | HEAD |
|---|---|
| HIGH: emoji undercount | Deferred to CLO-921 |
| MEDIUM: omission marker causes false terminal rejection | Fixed; exact-fit regression added |
| LOW: warnings omit truncated untracked files | Fixed; regression added |
| Missing terminal-overflow → successful fallback test | Added and passing |
| Gemini/OpenAI windows lack primary sources | Remains a documented limitation |

No additional material correctness, regression, or security defects were found within the reviewed scope. No FFI changes were introduced.

## Missing Items

- A regression covering the terminal rung under a byte ceiling smaller than its omission marker.
- Provider-published sources for the Gemini/OpenAI window entries remain outstanding, as documented.

AC1–AC4 have coverage under the approved heuristic policy; they do not establish a strict token upper bound.

## Recommendations

- Drop the omission marker when it exceeds either the token allowance **or** the byte ceiling.
- Ensure CLO-921 covers Unicode-heavy inventories, completion reserves, and output-cap checks.
- Replace aggregator window citations when primary evidence becomes available.

Validation passed: formatting, Clippy with warnings denied, the full test suite, all 11 grouping-budget integration tests, the unchanged public-surface check, and the library build without default features. Both signing-dependent integration cases executed without skipping. Rustdoc reported five warnings in unchanged files. The working tree remains unchanged.