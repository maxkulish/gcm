# Spec Review: clo-514

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-06-23
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is clear, complete, and highly accurate. It perfectly aligns with the Linear task description for **CLO-514** and the goals of **FR-60**. It correctly identifies the critical security weakness in the current `src/privacy.rs` implementation (the failure of the hand-coded keyword-based scanner to catch prefix-less, generically named credentials like `GITLAB="..."`) and proposes a robust, data-driven replacement. The API integration boundaries (`secret_ranges` returning `Vec<Range<usize>>`) are properly preserved to maintain seamless integration with `scan_text` and `redact_secrets`.

## 2. Acceptance Criteria Review

**Strong**:
- **Excellent Scope**: Covers specific secret formats (AC2), specific false-positive scenarios (AC3), inline pragmas (AC4), and dependency budgets (AC6).
- **Testable Thresholds**: Sets clear, charset-aware Shannon entropy targets (base64 ≈ 4.5, hex ≈ 3.0, mixed ≈ 3.5) with a length gate of 16 characters.

**Gaps**:
- **AC4 (Pragma Generality)**: Restricting pragma comments to `# gcm:allow` or `// gcm:allow` misses common formats such as SQL (`-- gcm:allow`), HTML (`<!-- gcm:allow -->`), or CSS/C-style (`/* gcm:allow */`). The AC should be generalized to detect the substring `gcm:allow` anywhere on the matching line.
- **UTF-8 Slicing Boundary Safety**: Since redaction slices string slices based on byte ranges returned by the regex engine, there is an implicit requirement that matches must fall on valid UTF-8 character boundaries to prevent runtime panics. This must be an explicit, testable criteria.

## 3. Constraints Check

**Aligned**:
- **Seam Preservation**: Preserving the signature of `secret_ranges()` returning `Vec<Range<usize>>` prevents regression risks at the integration points.
- **Single Compilation**: Requiring TOML compilation to happen exactly once (during `Privacy::load`) avoids unnecessary synchronization or global static `OnceLock` state, as the `Privacy` reference is threaded through the application's runtime.
- **Zero-Dependency Budget**: Restricting additions to only the pure-Rust `regex` crate and hand-rolling Shannon entropy aligns perfectly with the codebase's lightweight footprint.

**Concerns**:
- **Lookarounds Incompatibility**: The pure-Rust `regex` crate does not support PCRE lookarounds (such as lookahead `(?=...)`/`(?!...)` or lookbehind `(?<=...)`/`(?<!...)`), which are heavily utilized in upstream Gitleaks TOML configs. The constraints must explicitly mandate that all transcribed rules are rewritten/refactored into standard, lookaround-free regex patterns.
- **Error Types**: The prompt and criteria should note that the codebase uses its own custom error enum (`GcmError`) rather than `anyhow`. Any parsing or compilation failure during rule-pack loading must map to a custom `GcmError::Config` variant.

## 4. Decomposition Quality

**Well-scoped**:
- **Incremental Tasks**: The 6-step decomposition is exceptionally logical and represents highly independent units of work scoped to under 2 hours.
- **Clear Critical Path**: Properly identifies that Engine Foundation (1) must precede the other tasks, and that Pipeline (3) and Generic Detector (4) lead into FP Controls (5).

**Issues**:
- No major issues. The division of labor between `rules.rs`, `entropy.rs`, and `detect.rs` is highly structured.

## 5. Evaluation Coverage

**Covered**:
- **Comprehensive Test Matrix**: Features a table-driven verification strategy covering every acceptance criterion, utilizing both unit tests and end-to-end regression scripts (`scripts/acceptance.sh`).

**Gaps**:
- **Diff Prefix Tolerance**: Missing an explicit test scenario to ensure that diff-added/deleted leading indicators (`+` or `-`) and leading indentation are correctly handled by the generic-assignment identifier regex (e.g., `+GITLAB="3cjcjg988jrskbxx"` must still match `GITLAB`).
- **Slicing Panic Test**: Missing an explicit test scenario to verify that slicing non-ASCII multi-byte UTF-8 characters (e.g., emojis or Cyrillic strings) adjacent to a redacted secret does not cause a thread panic.

## 6. Codebase Alignment

**Violations**:
- None.

**Alignment**:
- **Module Split**: The proposed structure (`src/privacy/mod.rs`, `rules.rs`, `entropy.rs`, `detect.rs`) mirrors the exact organization of the `src/provider/` modules.
- **Error Taxonomy**: Placing TOML parsing and Regex compilation errors under `GcmError::Config` matches the standard configuration loading flow.
- **Lifecycle Preservation**: Passing the compiled engine as part of the `Privacy` struct (and referencing it via `&Privacy`) matches the borrow lifetimes managed in `src/main.rs`.

## 7. Blind Spots

- **Lookaround Replacements**: Upstream Gitleaks rules often use negative lookaheads to avoid matching substrings in larger tokens. The lack of lookarounds in the `regex` crate must be bypassed by implementing secondary validation logic in Rust (e.g., checking word boundaries `` or explicitly testing matched capture groups against exception lists).
- **Character-Based Entropy**: Shannon entropy computed over raw bytes will over-inflate the entropy value of strings containing multi-byte UTF-8 sequences (such as non-English alphabets or symbols). The hand-rolled calculation must iterate over `.chars()` rather than `.bytes()`.
- **Charset Classification Order**: Since hex characters (`[0-9a-fA-F]`) are a subset of base64 characters (`[A-Za-z0-9+/=]`), the charset classifier must check for hex first. If base64 is checked first, hex tokens will be classified as base64, causing them to fail to clear the higher 4.5 base64 threshold.
- **Git Metadata/Header Exclusion**: Git metadata lines in a diff (e.g., `index 0123456..789abcd 100644`) contain high-entropy hex sequences. The scanner must skip or exclude lines matching git headers before running the generic-assignment detector.
- **Line-to-Path Attribution Implementation**: Since `scan_text` only receives a concatenated text string, attributing lines back to paths (for lockfile/fixture exclusion) requires a stateful line-by-line parser that tracks `diff --git a/` or `+++ b/` prefixes to update the active filename context.

## 8. Verdict

**Verdict**: APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback

1. **Mandate Lookaround-Free Rules**: Update **§3 Constraints** to explicitly forbid PCRE lookarounds in `rules.toml`. Add a requirement to rewrite rules using standard word boundaries (``) or handle secondary exclusions via Rust filters in `detect.rs`.
2. **Generalize AC4 (Pragma Bypass)**: Rewrite AC4 to detect any line containing the substring `gcm:allow` (case-insensitive) rather than checking only `#` or `//` comments, ensuring multi-language compatibility (SQL, HTML, C-style comments).
3. **Specify Character-Based Entropy**: Update **§3 Constraints (Entropy)** to mandate that the hand-rolled Shannon entropy function iterates over the input's unicode scalar values (`char`s), not raw bytes, to prevent multi-byte characters from artificially inflating the score.
4. **Specify Charset Check Order**: Add a constraint in **§3 (Entropy)** requiring that charset classification checks for **Hex** membership before falling back to **Base64** or **Mixed** classification.
5. **Add Git Metadata and Diff Prefix Tolerances**: Update **§3 Constraints (Must)** to require that the generic-assignment regex:
   - Ignores lines that represent git metadata headers (e.g., `index ...`, `similarity ...`).
   - Gracefully ignores leading diff indicator prefixes (`+` or `-`) when capturing identifier names.
6. **Add UTF-8 Boundary Safe-Slicing Test**: Add an explicit edge case test in **§5 Evaluation** to verify that a secret embedded alongside multi-byte characters (such as Cyrillic text or emojis) is redacted safely without triggering UTF-8 slicing panics.
