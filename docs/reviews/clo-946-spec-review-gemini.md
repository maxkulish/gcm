# Spec Review: clo-946

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Reviewed**: 2026-11-20
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is clear, self-contained, and matches the Linear issue (`CLO-946`). It traces the defect to the `END` block in `.lok/lib/review-guard.sh:99-125`, names all four caller sites (`.lok/lib/review-guard.sh:188-190`, `.lok/lib/review-guard.sh:196-206`, and the synthesis branches in `.lok/workflows/spec-review.toml:343-350`, `.lok/workflows/design-review.toml:384-391`, and `.lok/workflows/pre-pr-validation.toml:423-430`), and links to the originating finding in `docs/reviews/clo-945-codex-validation.md:15`.

Two strengths worth calling out:
- **Explicit assumption on `FAIL` reports**: The spec states upfront that `review_usable` gates usability, not pass/fail, so a consistent `FAIL` review stays usable (`exit 0`) and its findings reach the orchestrator.
- **Verified impact on stored reports**: Checking `docs/reviews/clo-537-codex-validation.md` confirms line 30 is `## Verdict: PASS | PASS_WITH_NOTES | FAIL` (P1 template), line 511 is bold inline text (`**Verdict: APPROVE_WITH_SUGGESTIONS**`, not a Markdown heading), and lines 10418 and 10440 are both `## Verdict: FAIL`. Ignoring the template line keeps CLO-537 usable with verdict `FAIL`, reconciling the ticket's CLO-537 check with its placeholder rule.

---

## 2. Acceptance Criteria Review

**Strong**:
- **AC1, AC3, AC4, AC6, AC7**: Concrete, testable rules for scanning all verdict headings outside code fences, collapsing repeated identical verdicts, flagging disagreements (`exit 3`), ignoring placeholders, and keeping trailing prose valid.
- **AC8**: Defines exact failure text (`conflicting verdicts` / `SYNTHESIS_FAILED: synthesis output has conflicting verdicts`) across `review_guard`, `review_classify`, and the three lok workflows while preserving all other non-zero exit messages.
- **AC9, AC10, AC11, AC13**: Preserve `vline` semantics for `prepr` heading counts, require zero regressions across `docs/reviews/*.md`, mandate the CLO-537 check in the PR body, and require dual-OS CI execution.

**Gaps**:
- **AC2 contradicts Edge Case 2 on `prepr` case sensitivity**: AC2 says `(the prepr phase compares upper-cased)`, whereas Edge Case 2 states `Lower-case pass in prepr is not a token (unchanged phase rules)`. In `.lok/lib/review-guard.sh:64-72`, `verdict_token` matches `PASS_WITH_NOTES`, `PASS`, and `FAIL` case-sensitively, and upper-cases only for `APPROVE_WITH_CHANGES`, `APPROVE`, and `REWORK`. AC2 should match that exact rule so `pass` is not accidentally accepted in `prepr`.
- **P3 condition-word boundary and separator precision**: Section 1 / AC1 defines P3 as `a token followed directly by a condition word: if, when, unless, otherwise (case-insensitive, after optional closing markdown)`. It does not state whether a non-word boundary is required after the condition word (e.g., `([^A-Za-z0-9_]|$)` so `PASS iffy` or `PASS whenever` is not misclassified while `PASS otherwise` at end-of-line still matches) or which closing markdown characters are stripped (`[*_`]*[ \t]+`).
- **AC8 vs AC13 / Decomposition #3 mismatch**: AC13 requires the committed test script to cover AC4-AC8, but AC8's synthesis-branch behavior in the three `.lok/workflows/*.toml` files is only tested manually in Evaluation #13, not in `.lok/tests/review-guard-test.sh`.

---

## 3. Constraints Check

**Aligned**:
- Requiring POSIX `sh` plus portable `awk` (macOS `one-true-awk` 20200816 and Ubuntu `mawk`/`gawk`, banning gawk extensions such as `gensub`, `asort`, `IGNORECASE`, `\y`).
- Reusing `verdict_token` and `single_verdict` to preserve the accepted token set and P1 detection.
- Preserving `exit 2` for unknown phases, preserving `exit 1` messages, keeping test fixtures self-contained in a temp directory, and leaving out-of-scope CLO-949 work untouched.

**Concerns**:
- **Portable `awk` array-length pitfall**: `length(array)` is a gawk extension and fails in macOS `one-true-awk` / strict POSIX `awk`. Adding a note (or constraint) to track distinct verdicts with an explicit scalar counter (`if (!(v in seen)) { seen[v] = 1; n_verdicts++ }`) avoids a CI failure on macOS.
- **Comment update scope in `.lok/lib/review-guard.sh`**: Decomposition #1 updates `review_usable`'s header comment (lines 6-8), while `review_classify`'s header comment (lines 16-18) also enumerates all printed failure reasons and should list `"conflicting verdicts"`.

---

## 4. Decomposition Quality

**Well-scoped**:
- Four focused sub-tasks (`Verdict scan`, `Named failure`, `Tests`, `Evidence`) with clear file boundaries (6 files total, matching the header estimate) and explicit dependencies (`2` after `1`; `3` parallel with `1` and verified after `1`+`2`; `4` last). Each sub-task fits comfortably within ~2 hours.

**Issues**:
- **Sub-task 3 omits workflow synthesis-branch verification**: `.lok/tests/review-guard-test.sh` covers Evaluation #1-#9, leaving Evaluation #13 (the synthesis branch edits in `spec-review.toml`, `design-review.toml`, and `pre-pr-validation.toml`) out of CI.
- **Sub-task 2 omits `review_classify` doc-comment update** in `.lok/lib/review-guard.sh:16-18`.

---

## 5. Evaluation Coverage

**Covered**:
- Evaluation #1-#14 and the 5 edge cases cover conflicting headings (`prepr`, `synthesis-prepr`), P1/P2/P3 (`if`, `when`) placeholders, repeated `PASS`/`FAIL`, placeholder + real verdict, trailing prose, `review_guard`/`review_classify` named failures, before/after sweep across `docs/reviews/*.md`, CLO-537 verification, CLO-946 spec-review safety, and `ubuntu-latest` + `macos-latest` CI.
- Red-first checks against `git show main:.lok/lib/review-guard.sh` are well-placed for AC4 and AC5.

**Gaps**:
- **P3 `unless` and `otherwise` have no fixtures**: AC5 only tests `if` and `when`. Add `unless` and `otherwise` cases (plus a negative boundary check such as `PASS iffy` if word boundaries are required).
- **Missing `synthesis-review` conflict test and committed `review`-phase test**: Evaluation #1-#3 test `prepr` and `synthesis-prepr`, while `synthesis-review` conflict (exit 3) is unexercised and Edge Case 5 (`review` phase `APPROVE` vs `APPROVE_WITH_SUGGESTIONS`) is not listed in the Evaluation table for the test script.
- **Missing placeholder + conflicting-verdicts fixture**: No case tests a report with a P1/P2/P3 placeholder heading followed by two conflicting real verdicts (must exit 3, confirming a leading placeholder does not suppress conflict detection), or a heading whose remainder is a placeholder followed on the next line by a real verdict (must exit 0 and set `vline`).
- **Evaluation #13 does not verify `spec-review.toml` or `design-review.toml` execution**: Only `pre-pr-validation.toml`'s branch logic is run in a scratch shell, and none of the three TOML changes run in CI.

---

## 6. Codebase Alignment

**Alignment**:
- **Rust codebase boundary**: `src/error.rs` (`GcmError`) and `src/provider/mod.rs` / `src/provider/facade.rs` confirm `gcm` is a synchronous `ureq`-based Rust CLI/library (the prompt's `Backend` trait / `anyhow` / `BackendErrorKind` references belong to `lok`'s generic spec-review template). Keeping all Rust source files untouched is appropriate.
- **Caller contracts**: Changing `review_guard` to emit `REVIEW_FAILED: <reviewer> <model>: conflicting verdicts` (still prefixed with `REVIEW_FAILED:` and exiting 0) keeps `.claude/commands/task/phases/implement.md:118-120` and `.claude/commands/design-doc/review.md:443-444` working without modification. Keeping `SYNTHESIS_FAILED: <reason>` as line 1 keeps `.lok/workflows/pre-pr-validation.toml:440-443` exiting 1 on conflict and preserves the check in `.claude/commands/task/phases/{spec,design}.md`.

**Violations**:
- None found.

---

## 7. Blind Spots

- **Pre-PR reports of CLO-946 quoting `PASS` and `FAIL` outside code fences**: AC12 notes that `clo-946-spec-review-*.md` runs in phase `review` (where `PASS` and `FAIL` are not tokens). During pre-PR validation of CLO-946 (`prepr` and `synthesis-prepr`), `PASS` and `FAIL` *are* valid tokens; if a pre-PR reviewer or synthesis puts `## Verdict: PASS` and `## Final Verdict: FAIL` at column 0 outside a code fence when summarizing the fix, `review_usable` will reject it as conflicting verdicts.
- **`single_verdict` upper-casing in `prepr`**: In `.lok/lib/review-guard.sh:84`, `single_verdict` unconditionally runs `if (phase == "prepr") s = toupper(s)` before matching `r`. The new value function should gate on `verdict_token` first (or replicate its split case rules) so lowercase `pass | fail` in `prepr` returns `""` rather than leaking through `toupper(s)`.
- **POSIX `sh` (`dash`) portability in `.lok/tests/review-guard-test.sh`**: On `ubuntu-latest`, `sh` is `dash` and `awk` is typically `mawk`. Ensure the test script itself avoids bashisms (`local`, `[[ ... ]]`, arrays) and validates both `review-guard.sh` and the extracted/checked synthesis-branch handling in CI.

---

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The problem diagnosis, owner decisions on placeholders and `FAIL` usability, exit-code contract (`0`/`1`/`2`/`3`), caller integration, and historical-report analysis are accurate and well-grounded in the codebase. Addressing the AC2 wording mismatch, P3 regex boundary precision, and the remaining test/CI coverage gaps will make the specification unambiguous to implement.

---

## 9. Actionable Feedback

1. **[High] Reconcile AC2 with Edge Case 2 on `prepr` case sensitivity**: Update AC2 (`docs/specs/2026-11-20-clo-946-verdict-check.md:37`) to state that `prepr` matches `PASS`, `PASS_WITH_NOTES`, and `FAIL` case-sensitively and upper-cases only `APPROVE_WITH_CHANGES`, `APPROVE`, and `REWORK`, matching `verdict_token` in `.lok/lib/review-guard.sh:64-72`.
2. **[Medium] Specify P3 boundary and markdown rules precisely**: In Section 1 / AC1, state that P3 matches `if|when|unless|otherwise` case-insensitively after optional closing markdown and horizontal whitespace (`[*_`]*[ \t]+`) and requires a non-word boundary afterwards (`([^A-Za-z0-9_]|$)`).
3. **[Medium] Expand Evaluation and `.lok/tests/review-guard-test.sh` coverage**:
   - Add P3 fixtures for `unless` and `otherwise`, a `synthesis-review` conflict fixture (`exit 3`), a `review`-phase conflict fixture (`APPROVE` vs `APPROVE_WITH_SUGGESTIONS`), and a fixture combining a placeholder heading with two conflicting real verdicts (`exit 3`).
   - Include a check in `.lok/tests/review-guard-test.sh` (or CI) that exercises the synthesis-branch `SYN_RC=3` mapping across all three `.lok/workflows/*.toml` files rather than leaving Evaluation #13 to a manual scratch shell on `pre-pr-validation.toml` only.
4. **[Low] Add portable-`awk` counter note and `review_classify` doc comment**: Note in Section 3 or Decomposition #1 not to use `length(array)` in `awk` (use a scalar counter for distinct verdicts), and include updating `review_classify`'s header comment (`.lok/lib/review-guard.sh:16-18`) in Decomposition #2.
