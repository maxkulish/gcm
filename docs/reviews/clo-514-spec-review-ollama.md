# Spec Review: clo-514

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-23
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Clear and Accurate.** The specification correctly identifies that `src/privacy.rs` contains a hand-coded best-effort secret scanner (`secret_ranges()`) with two detectors:
- `prefixed_token_ranges` — hardcoded prefix matching (AWS `AKIA`/`ASIA`, GitHub `ghp_`/`gho_`/etc., `sk-`)
- `assignment_value_ranges` — keyword-allowlist matching (`api_key`, `secret`, `token`, etc.)

The concrete failure (`GITLAB="3cjcjg988jrskbxx"` passing through) is well-articulated and matches the Linear description. The proposed solution (data-driven TOML rule pack + entropy detection) directly addresses the limitation.

**Matches Linear.** The Linear task description and the spec are aligned on:
- Replacing hand-coded matcher with vendored TOML rule pack (gitleaks MIT + Kingfisher Apache-2.0)
- Pure-Rust `regex` crate execution
- Charset-aware Shannon entropy gating
- Catching prefix-less credentials without false-positive floods

**No Unstated Assumptions.** The spec correctly references `src/privacy.rs:248`, `src/privacy.rs:278`, and `src/privacy.rs:300` which are verified to exist and contain the described code.

## 2. Acceptance Criteria Review

**Strong:**
- **AC1 (generic credential caught)** — Clear, testable with `GITLAB="..."` example
- **AC2 (vendored providers detected)** — Enumerates all providers with specific token patterns
- **AC3 (false-positive controls)** — Specific examples (UUID, git SHA, SHA-256, npm integrity)
- **AC4 (pragma honored)** — Specific syntax (`# gcm:allow`, `// gcm:allow`)
- **AC5 (data-driven)** — TOML edit without Rust change is testable
- **AC7 (no regression)** — Existing tests must pass, clear verification method

**Gaps:**
1. **AC6 (dependency budget)** says "Exactly one new runtime crate (`regex`)" but the spec also mentions `serde` for TOML parsing. The Cargo.toml already has `serde` and `serde_json`, so this is technically correct (no *new* serde needed), but could be clearer that TOML parsing uses the existing `toml` crate (already present in Cargo.toml).

2. **Missing criterion for compile-time validation.** The "Must-not" constraint says "Panic on a malformed vendored rule... a compile-time-vendored bad regex should be caught by a test" — but there's no AC for this test existing. Should be an explicit test criterion.

3. **Missing criterion for UTF-8/byte boundary handling.** Edge case 11 mentions multi-byte/UTF-8 input must not panic on byte-range slicing, but no AC covers this explicitly.

## 3. Constraints Check

**Aligned with Codebase:**
- The `Vec<Range<usize>>` seam (`secret_ranges`) is verified to exist in `src/privacy.rs:248` and called by `scan_text` and `redact_secrets`
- The `GcmError::SecretDetected { count }` variant exists in `src/error.rs`
- `SecretScanMode` enum (Off/Redact/Abort) matches the existing implementation
- The `Privacy::load()` and integration pattern is consistent with existing code

**Correct Categories:**
- **Must** — Preserving the `Vec<Range<usize>>` seam is correctly critical
- **Must-not** — Only one new runtime crate is correctly strict (no Hyperscan, rayon, network calls)
- **Prefer** — Module split is appropriately preference-level, not requirement
- **Escalate** — Line→path attribution and regex feature support are good escalation triggers

**Concerns:**
1. **The "Must" for entropy thresholds** specifies exact values (base64-class ≈ 4.5, hex-class ≈ 3.0) but the spec says "These defaults live as named constants and are documented inline" — this is good, but the actual values need validation against real-world secret patterns vs UUID/git-SHA distributions. Consider adding a "Prefer" to tune these based on false-positive/negative testing.

2. **The "Must-not" about user-supplied external file** is overly restrictive. While deferring `--secret-rules <path>` is correct, the spec should clarify that the TOML schema *not precluding* this future flag doesn't mean the current implementation has hooks for it — it's purely schema design.

3. **Missing constraint:** The current `privacy.rs` module is a single ~350-line file. The proposed split into `src/privacy/{mod,rules,entropy,detect}.rs` is a significant structural change that should have a "Must" or "Prefer" about maintaining backward compatibility for the public API (`Privacy::load`, `filter_changed`, `prepare_grouping`, `prepare_diff`).

## 4. Decomposition Quality

**Well-scoped:**
- Sub-task 1 (Engine foundation) — `regex` dependency, module split, TOML schema, entropy fn — appropriate for ~2 hours
- Sub-task 2 (Vendor rule pack) — Transcribing gitleaks/Kingfisher rules — appropriate
- Sub-task 3 (Detection pipeline) — Keyword prefilter → RegexSet → capture → gate — clear scope
- Sub-task 4 (Generic assignment + entropy) — The core `GITLAB="..."` fix — appropriately focused

**Issues:**
1. **Sub-task 5 (False-positive + pragma) combines two distinct concerns.** Consider splitting:
   - 5a: FP suppression (UUID/git-SHA/integrity, `min_digits`, entropy ignore-set)
   - 5b: Pragma handling (`# gcm:allow`, `// gcm:allow`)
   
   These are logically separable and could be parallelized.

2. **Missing sub-task for migration of existing prefix detectors.** Sub-task 3 mentions "Migrate the old prefix detectors onto rules (no regression)" but this is buried. Should be explicit: verify that existing `prefixed_token_ranges` detections (AWS `AKIA`, GitHub `ghp_`, `sk-`) are covered by the new rule pack with test coverage.

3. **Dependency order** correctly identifies 1→{2,3,4}→5→6, but sub-task 2 (vendor rules) should explicitly state "depends on schema from sub-task 1" in its description, not just the summary.

## 5. Evaluation Coverage

**Covered:**
- Test table has 11 rows covering AC1–AC7
- Edge cases section has 6 items covering overlapping ranges, pragma edge cases, UTF-8

**Gaps:**
1. **Missing test for "keyword-named assignment uses lower-entropy fast path."** The spec mentions this optimization in §3 (Must), but there's no test case verifying that `API_KEY=lowentropyvalue` (keyword in allowlist, low entropy) is detected via fast path while `RANDOM_IDENT=lowentropyvalue` is not. This is a behavioral distinction that should be verified.

2. **Missing test for entropy ignore-set interaction with precise rules.** Edge case 11 covers `glpat-...` inside `package-lock.json`, but the test table lacks a row. Add: "A real `glpat-...` inside a lockfile is caught by its precise rule despite the ignore-set."

3. **Missing performance test.** The "Must" says "Compile the rule pack **once**, not per scanned string (a single gcm run calls the scanner ~6 times)." This needs a benchmark or at least an assertion that `RegexSet` compilation is cached. No test row for this.

4. **Missing test for empty/minimal TOML.** What happens if `rules.toml` contains only comments/attribution header? Should gracefully degrade (no detections) rather than panic. This is a forward-compatibility concern.

## 6. Codebase Alignment

**Violations: None found.** The spec correctly follows established patterns:

1. **Error handling:** Uses existing `GcmError::SecretDetected { count }` variant, matches `anyhow`-style error taxonomy in `src/error.rs`

2. **Module structure:** Proposed `src/privacy/` directory mirrors existing `src/provider/` precedent (mod.rs + submodules)

3. **API contract:** Preserves `Privacy::load()`, `filter_changed()`, `prepare_grouping()`, `prepare_diff()` — verified these are the only public functions in `src/privacy.rs`

4. **Testing approach:** Unit tests in `#[cfg(test)]` modules within each file, plus `scripts/acceptance.sh` extension — matches existing pattern (see `privacy.rs` tests and `acceptance.sh` AC-S1/S2/S3)

5. **Dependency management:** `regex` crate is a reasonable addition; already has `serde`, `toml` for TOML parsing

**Alignment Observations:**
- The spec correctly identifies that `toml` crate is already a dependency (Cargo.toml line 12: `toml = "0.8"`)
- The existing `privacy.rs` has comprehensive tests including `abort_mode_rejects_secret_text` — the new tests should extend this module

## 7. Blind Spots

**What the specification misses:**

1. **Error message improvement.** Current `GcmError::SecretDetected { count }` shows a generic message. The new engine could provide *which* rule matched (e.g., "AWS secret key" vs "GitHub PAT" vs "generic assignment"). This would help users understand what to fix. Not blocking, but worth considering.

2. **Performance impact of `RegexSet` on cold start.** The spec requires single compilation, but doesn't estimate the compilation time. A rule pack with 20+ regexes could add noticeable latency. Consider:
   - Lazy initialization via `OnceLock`
   - Benchmark target in the "Must" constraint

3. **Rule conflict resolution.** What happens when a single secret matches multiple rules (e.g., a high-entropy string that matches both a specific provider pattern and the generic entropy detector)? The spec mentions `merge_ranges`, but not rule priority/precedence. Should specific rules take precedence over generic entropy?

4. **Logging/debugging visibility.** When `--secret-scan=abort` fires, the user only sees the count. For debugging, it would be valuable to log which rule(s) matched. Consider adding `GCM_LOG_LEVEL=debug` integration showing matched rules.

5. **Lockfile path attribution.** The "Escalate when" trigger correctly identifies that attributing a diff line to its source file is hard for concatenated text. But the spec doesn't propose *how* to handle this for `prepare_grouping` which receives a `GroupingContext` (file_list, status, stat, body) — the body is already concatenated. This needs a design decision: either pass file paths alongside body (breaking change) or accept imprecision for `prepare_grouping` entropy ignore-set.

6. **Secret rotation false positives.** A commit that *removes* a secret (replaces `API_KEY=oldvalue` with `API_KEY=newvalue`) will still match in both old and new form. This is arguably correct behavior, but should be documented. Users might be confused why their "secret removal" commit is blocked.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is thorough, well-aligned with the codebase, and addresses a real security gap. The decomposition is sound and the acceptance criteria are mostly testable. However, there are several gaps that should be addressed before implementation:

1. **Missing AC for compile-time validation** of vendored rules
2. **Missing test for entropy ignore-set + precise rule interaction**
3. **Sub-task 5 should be split** (FP suppression vs pragma handling)
4. **Performance/logging blind spots** need acknowledgment

## 9. Actionable Feedback

**High Priority (address before implementation):**

1. **Add AC8: Rule pack validation at compile time.** "A malformed regex in `rules.toml` fails to compile with a clear error message, not a runtime panic. Unit test in `rules.rs` verifies all vendored patterns compile."

2. **Split Sub-task 5** into 5a (FP suppression) and 5b (Pragma handling). This allows parallel implementation and clearer scope.

3. **Add test row for entropy ignore-set + precise rule.** "A real `glpat-...` inside `package-lock.json` is caught by its precise rule despite the entropy ignore-set."

4. **Clarify line→path attribution design.** The "Escalate when" trigger acknowledges this is hard. Either:
   - Accept that `prepare_grouping` cannot use path-based ignore sets (only `prepare_diff` can), OR
   - Propose passing file boundaries alongside body text

**Medium Priority (address during implementation):**

5. **Add logging integration.** When `GCM_LOG_LEVEL=debug`, log which rule(s) matched. This aids debugging without changing user-facing behavior.

6. **Document rule priority.** Clarify in §3 "Must" that specific (named-provider) rules take precedence over generic entropy detection when ranges overlap.

7. **Add test for empty/minimal rules.toml.** Verify graceful degradation rather than panic.

**Low Priority (consider for future):**

8. **Consider error message enhancement.** Show which rule matched (e.g., "AWS secret key" vs "generic high-entropy") in the `SecretDetected` message or debug log.

9. **Add benchmark target.** "Rule pack compilation takes < X ms" to prevent pathological regex patterns from impacting cold start.

10. **Document removal detection behavior.** Explicitly note that removing a secret still triggers detection (both old and new values match), which is correct but may confuse users.
