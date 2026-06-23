# Spec Review Synthesis: clo-514

**Synthesized**: 2026-06-23
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | Problem statement is clear, accurate, and matches the Linear CLO-514 description (the `GITLAB="..."` failure of the keyword scanner is well-articulated) | Info |
| 2 | Acceptance criteria are strong on scope and testability (specific secret formats, FP scenarios, dependency budget, entropy thresholds) | Info |
| 3 | Preserving the `secret_ranges() -> Vec<Range<usize>>` seam is correct and prevents regression at `scan_text`/`redact_secrets` integration points | Info |
| 4 | Rule pack must compile **once** (at `Privacy::load`), not per scanned string; no test/assertion currently verifies this caching | Medium |
| 5 | Dependency budget (one pure-Rust `regex` crate, hand-rolled entropy; `toml`/`serde` already present) aligns with the codebase footprint | Info |
| 6 | Module split `src/privacy/{mod,rules,entropy,detect}.rs` correctly mirrors the existing `src/provider/` structure | Info |
| 7 | **UTF-8 byte-boundary slicing safety** is an implicit requirement with no explicit AC/test; multi-byte input (Cyrillic, emoji) adjacent to a secret could panic | High |
| 8 | **Line→path attribution** (for lockfile/fixture ignore-sets) is genuinely hard because `scan_text`/`prepare_grouping` receive concatenated text with no file boundaries; needs a design decision | High |
| 9 | Decomposition is well-scoped with a correct critical path (1 → {2,3,4} → 5 → 6) | Info |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | Error taxonomy | Codebase uses a custom `GcmError` enum, **not** `anyhow`; parse/compile failures map to `GcmError::Config` | Describes `src/error.rs` as "anyhow-style", focuses on existing `GcmError::SecretDetected` | SKIPPED |
| 2 | Pragma scope (AC4) | Too narrow — generalize to detect substring `gcm:allow` anywhere on a line (SQL `--`, HTML `<!-- -->`, C-style `/* */`) | Accepts `# gcm:allow` / `// gcm:allow` as specified and testable | SKIPPED |

> **Note on #1:** This is a factual conflict about whether the codebase uses `anyhow` or a custom `GcmError`. Verify against `src/error.rs` before locking the spec — it changes how rule-load failures are typed. The actual repo evidence (Ollama cites `GcmError::SecretDetected` existing) suggests a custom enum, supporting Gemini's `GcmError::Config` recommendation.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Regex crate has no lookarounds** — Gitleaks TOML rules rely on `(?=)`/`(?!)`; all transcribed rules must be rewritten lookaround-free, with secondary exclusion logic in `detect.rs` | Gemini | High |
| 2 | Shannon entropy must iterate `.chars()` not `.bytes()` — multi-byte UTF-8 sequences inflate the score | Gemini | High |
| 3 | Charset classifier must check **hex before base64** (hex ⊂ base64), else hex tokens get the stricter 4.5 threshold and slip through | Gemini | High |
| 4 | Git metadata lines (`index 0123456..789abcd`, `similarity`) carry high-entropy hex and must be skipped before the generic detector runs | Gemini | High |
| 5 | Diff prefix tolerance — `+GITLAB="..."` / leading indentation must still capture the `GITLAB` identifier; no test covers this | Gemini | Medium |
| 6 | No AC for **compile-time validation** of vendored rules — a malformed regex should fail a test with a clear error, not panic at runtime | Ollama | High |
| 7 | Rule conflict resolution / precedence is unspecified — when a token matches both a precise provider rule and generic entropy, which wins? (affects `merge_ranges`) | Ollama | Medium |
| 8 | Migration of old prefix detectors (AWS `AKIA`, GitHub `ghp_`, `sk-`) onto the new rule pack is buried in sub-task 3; should be explicit with regression coverage | Ollama | Medium |
| 9 | No test for empty/minimal `rules.toml` (only attribution header) — should degrade gracefully, not panic | Ollama | Medium |
| 10 | No test for keyword fast-path distinction (`API_KEY=lowentropy` detected vs `RANDOM_IDENT=lowentropy` not) | Ollama | Medium |
| 11 | Suggest splitting sub-task 5 into 5a (FP suppression) + 5b (pragma) for parallelism | Ollama | Low |
| 12 | Debug logging of *which* rule matched, and richer `SecretDetected` messages, would aid users; document that secret-removal commits still trigger detection | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (both reviewers concur; Claude fallback skipped because external reviewers succeeded.)

The spec is sound and codebase-aligned. None of the findings block implementation, but several (regex lookarounds, char-based entropy, charset order, compile-time validation, UTF-8 slicing, line→path attribution) will cause real bugs or panics if not addressed in the spec before coding.

## Priority Actions

**Resolve first (agreed + high-severity):**
1. Add explicit AC + test for **UTF-8 byte-boundary safe slicing** (multi-byte chars adjacent to a redacted secret) — *both reviewers*.
2. Make a **line→path attribution** design decision: either accept that `prepare_grouping` cannot use path-based ignore-sets (only `prepare_diff` can), or pass file boundaries alongside the body — *both reviewers*.
3. Add a test/assertion that the rule pack **compiles once** and is cached on `Privacy` — *both reviewers*.

**High-confidence single-source correctness items (fold into §3 Constraints):**
4. Mandate **lookaround-free regex** in `rules.toml`; push exclusions into Rust filters (Gemini).
5. Specify entropy over **`.chars()`** not bytes, and **hex-before-base64** charset ordering (Gemini).
6. Require the generic detector to **skip git metadata headers** and tolerate **diff `+`/`-` prefixes** (Gemini).
7. Add **AC8: compile-time rule validation** — malformed vendored regex fails a unit test, never panics at runtime (Ollama).

**Resolve the two disagreements (need human/codebase check):**
8. Confirm error taxonomy against `src/error.rs` (custom `GcmError::Config` vs `anyhow`) and fix the spec accordingly.
9. Decide whether AC4 pragma detection generalizes to the `gcm:allow` substring (multi-language comments) or stays `#`/`//` only.

**Lower priority (tighten during implementation):**
10. Specify rule precedence (precise > generic entropy) for overlapping ranges.
11. Make old-prefix-detector migration an explicit, test-covered step.
12. Add tests for empty `rules.toml`, keyword fast-path, and lockfile-vs-precise-rule interaction; consider splitting sub-task 5; add debug logging of matched rule names.
