## Verdict: FAIL

The main implementation is present and all required checks pass, but the estimator and terminal degradation rung have correctness defects.

## Findings

1. **HIGH — Unicode estimation can substantially undercount tokens.** [src/budget.rs:37](/Users/mk/Code/gcm--fix-clo-797-grouping-prompt/src/budget.rs:37) counts one token per Unicode character. Against the [published o200k vocabulary](https://openaipublic.blob.core.windows.net/encodings/o200k_base.tiktoken), 100 repeated `🫠` characters require 300 tokens; this code estimates 100. Unicode-heavy content can therefore pass pre-flight and still exceed the context window. This follows D4 literally, but exposes an unsafe assumption in the design itself.

2. **MEDIUM — The paths-only rung can reject a prompt that fits.** [src/diff.rs:279](/Users/mk/Code/gcm--fix-clo-797-grouping-prompt/src/diff.rs:279) replaces the body with `BODY_OMITTED` rather than dropping it. I reproduced a 7,019-token window leaving exactly 63 tokens for the inventory, stat and rendered labels. Those fit with an empty body, but the marker raises the estimate to 95 and triggers fallback. D3/AC4 defines terminal failure by whether inventory plus stat fit.

3. **LOW — Degradation warnings omit reduced untracked files.** [src/diff.rs:340](/Users/mk/Code/gcm--fix-clo-797-grouping-prompt/src/diff.rs:340) calculates `reduced_files` from tracked sections only. With 20 untracked files, I reproduced: `cut 0 of 20 file diff(s) to 512 bytes each`, although their contents were truncated. This makes D6 diagnostics misleading for the original problem’s workload.

## Missing Items

- **ST7:** No test covers terminal grouping window overflow followed by a successful message fallback. The Anthropic success test exercises output-cap rejection, a separate branch.
- **ST3:** Gemini and OpenAI window entries cite aggregators rather than the required provider-published limits. See [src/budget.rs:111](/Users/mk/Code/gcm--fix-clo-797-grouping-prompt/src/budget.rs:111).

## Recommendations

- Use a conservative byte-based allowance for non-ASCII content and add Unicode-heavy budget regressions.
- Make the final ladder attempt use an empty body; test the exact-fit boundary.
- Include untracked truncations in the reduced-file count.
- Add the missing fallback case and verify window entries against primary provider sources.

Validation passed: formatting, Clippy with warnings denied, the full test suite, the public-surface script, and the library build without default features. Rustdoc emitted five warnings in unchanged files. The working tree remains unchanged.