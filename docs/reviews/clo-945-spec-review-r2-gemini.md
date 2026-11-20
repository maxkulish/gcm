# Spec Review (Round 2): clo-945

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Reviewed spec sha256**: 5c434bc8f048
**Date**: 2026-11-19

---

## 1. Problem Statement Assessment

The problem statement is clear, self-contained, and matches the Linear task description (`CLO-945`). It accurately documents:
- The replacement of Ollama and AI Studio with Codex (`gpt-6.2-sol`, `model_reasoning_effort='"high"'`) and Gemini (`google-vertex/gemini-4-hydrogen#high` via `opencode`).
- The 17 verified root causes across `.lok/workflows/spec-review.toml`, `.lok/workflows/design-review.toml`, `.lok/workflows/pre-pr-validation.toml`, `.lok/lok.toml`, `.lok/prompts/`, and `.claude/commands/task/phases/implement.md` (including the 12 additional defects surfaced across the two prior reviews, all cross-checked against the files in this repo).
- The Pi compatibility boundary, noting why `.pi/` does not require changes.

One minor note: the scope line (`12 files: 11 modified, 1 new; 6 sub-tasks`) matches Section 4 and Section 3's escalation threshold accurately.

---

## 2. Acceptance Criteria Review

**Strong**:
- **AC1–AC5, AC10–AC13, AC15, AC19–AC24** are concrete, testable, and close every gap from the prior reviews: `plan` agent permission lockdown with `--standalone` and `OPENCODE_CONFIG_CONTENT` (AC2, AC20), `shell_escape` data passing for both step outputs and workflow arguments (AC21), `.lok/local/review-input/<slug>-branch.diff` for read-only Gemini in pre-PR (AC23), AC24 structured design-review headings, and two-way Pi compatibility including legacy `review_ollama` resume (AC15, AC22).
- **AC8 & AC18** clearly separate the single post-validation usability decision (`review_status` step with prefix-only `REVIEW_FAILED:` matching and phase-specific structural/verdict checks) from downstream consumers.
- **AC14** deterministically handles zero-review synthesis (`NO_REVIEWS_AVAILABLE`) and forces a non-zero `lok` exit in `pre-pr-validation.toml` via `verdict_gate`.

**Gaps**:
- **AC9 reason precedence when the guard already emitted `REVIEW_FAILED:`**: When a reviewer CLI fails (e.g., `exit 1` or `exit 124`), the shell guard emits `REVIEW_FAILED: <reviewer> <model>: <reason>` and exits `0`. Downstream `[steps.validate]` will then reject that output (either via `min_length(200)` if stderr is short, or via Haiku's `NOISE: lines starting with REVIEW_FAILED`), leaving `steps.X.success == false`. AC9's `"Otherwise they write a failure report: REVIEW_FAILED: ... (reason validator rejected output, not a usable report, or step failed ...)"` can be read as overwriting the guard's specific `exit 1` / `exit 124` reason with `validator rejected output`, which would contradict AC6 and Evaluation #4/#6. State explicitly that an existing `REVIEW_FAILED:` prefix from the guard is preserved over `validator rejected output` / `step failed`.
- **AC9 stderr excerpt availability in write steps**: AC9 requires `write_reviews` / `write_reports` to append *"the step's stderr excerpt if any"* when writing `validator rejected output`, `not a usable report`, or `step failed`. Because the write step runs in a separate `lok` shell process, it cannot locate a random `mktemp` stderr file created inside the reviewer step (whereas AC16 only specifies `mktemp` for the inline gate). The spec needs to specify a deterministic per-slug, per-reviewer stderr path (e.g., `.lok/local/review-input/<slug>-<reviewer>.stderr`, cleared by `health_check`) for the three workflows.
- **AC8 missing `min_deps_success = 0` on `review_status`**: AC8 states `review_status` depends on both external reviewers, but does not require `min_deps_success = 0` (and `continue_on_error = true` or `exit 0`). Without `min_deps_success = 0`, `lok` will skip `review_status` whenever either reviewer step fails validation, breaking `claude_fallback`, `synthesis`, and `write_reviews`.
- **AC16 temp files vs direct `-o` / redirection**: AC16 does not explicitly state that `implement.md` writes Codex (`-o`) and Gemini outputs to temp files first and removes stale report files before running, rather than letting `codex exec -o` write directly to `docs/reviews/clo-XX-codex-validation.md`.

---

## 3. Constraints Check

**Aligned**:
- Keeping the workflow topology (`health_check` -> parallel external reviewers + `[steps.validate]` -> `review_status` -> `claude_fallback` -> `synthesis` with `continue_on_error = true` -> write step -> `verdict_gate` in pre-PR) matches `.lok/workflows/*.toml`.
- Using POSIX `sh` (`RC=0; OUTPUT=$(...) || RC=$?`, dot-sourcing `.`) matches `lok`'s `sh -c` runner.
- Guarding all skippable step interpolations (`{% if steps.X is defined %}`) aligns with `.pi/lessons/lok-workflow-templates.md` L1.
- Excluding `.pi/`, historical `docs/reviews/` / `docs/status/`, `lok` source, and `gcm` Rust source keeps the change bounded.

**Concerns**:
- **`[steps.validate]` retries on `REVIEW_FAILED:`**: The existing reviewer steps set `retries = 3` (or `2` for Codex pre-PR) alongside `[steps.validate]`. When the shell guard emits `REVIEW_FAILED:` (e.g., on `health_check` short-circuit, `exit 1`, or a 240s `exit 124` timeout), `[steps.validate]` rejects it, causing `lok` to retry the entire step up to 3 more times. For a 240s child timeout inside a `5m` (`300s`) `lok` step, retrying immediately pushes attempt 2 past the `5m` step deadline and can cause `lok` to kill the step before the guard's `exit 124` output is kept. Either note this interaction as a constraint/check (e.g., whether `retries` must be adjusted or how `lok` step timeout applies across retries) or verify it in Evaluation #6.
- **AC20 literal vs `OPENCODE_CONFIG_CONTENT` visibility in workflows**: Constraints and AC20 state the permission JSON lives once in `.lok/lib/review-guard.sh` and once in `implement.md`, while Evaluation #3 requires `OPENCODE_CONFIG_CONTENT` to match in all 3 workflows. Clarify that the workflows pass `OPENCODE_CONFIG_CONTENT="$GEMINI_PERMISSION_CONFIG"` (or re-export it visibly) so Evaluation #3 passes without duplicating the JSON literal.

---

## 4. Decomposition Quality

**Well-scoped**:
- Splitting into 6 sub-tasks (shared guard, `spec-review.toml`, `design-review.toml`, `pre-pr-validation.toml`, `implement.md` + `lok.toml`, and phase docs/templates) separates independent workstreams cleanly.
- Dependency order (1 before 2-5; 2-5 independent; 6 after 2-3) is accurate and each sub-task is realistically sized at ~1-2 hours.

**Issues**:
- **Sub-task 5 (`implement.md`)**: Should explicitly note capturing Codex and Gemini stdout/outputs to temp files (not directly to `docs/reviews/...` via `codex exec -o`), clearing stale reports, replacing the `Run: git diff main...HEAD` line in `implement.md`'s inline prompt with `.lok/local/review-input/<slug>-branch.diff`, and writing `**Reviewer**` headers (AC10) / failure reports (AC9).
- **Sub-task 6 (`.claude/commands/design-doc/review.md` and `orchestrate.md`)**:
  - `.claude/commands/design-doc/review.md` also mentions `GEMINI_FALLBACK_MODEL` in prose (lines 25, 218, 408) and `Gemini 3.1 Pro` / `gemini-3.1-pro-preview` (lines 249, 402, 450), which Evaluation #1 checks for; note removing those too.
  - AC14 requires a distinct history action when the inline gate is overridden at 0 usable reports; Sub-task 6 should list adding that action type to `.claude/commands/task/orchestrate.md`'s History Action Types table and `.claude/templates/workflow-state.yaml`'s history comment block.

---

## 5. Evaluation Coverage

**Covered**:
- Strong coverage across static `rg` checks (#1-#3), stub-CLI failure and combination tests (#4-#8d), live runs (#9-#12, #15, #18), TOML/lok validation (#13), direct AC18 helper unit checks (#14), heredoc/argument injection tests (#16, #19), and two-way Pi handoff (#17).

**Gaps**:
- **Evaluation #1 multiline `--dir` regex bug**: `opencode[^\n]*--dir\b` only matches `--dir` on the same line as `opencode`. All three workflows previously wrapped `opencode run "$PROMPT" \` across multiple lines with `--dir "$PWD"` on a later line, so `opencode[^\n]*--dir\b` would miss a multiline `--dir`. Use multiline `rg -nU` or flag `--dir\b` on any line not containing `lok run`.
- **Evaluation #3 `.lok/lok.toml` check**: Only checks `google-vertex/gemini-4-hydrogen#high`, missing checks for `--auto` in `[backends.gemini]` `args` (AC3) and `enabled = false` in `[backends.ollama]` (AC5).
- **Inline gate failure/branch paths missing from Evaluation**: Evaluation #11 only tests the live happy path on a specification task. There is no test verifying `implement.md` when: (a) Codex fails (`codex_validated: false`), (b) both fail (0 usable reports -> only `override`/`pause` offered), or (c) `task_type` is `development` (pointing at `design_doc` + `plan_file`).
- **Edge Case 2 (`Usage:` / `Unrecognized flag` in prose) not in test table**: Mentioned in Edge cases to verify, but has no stub test row confirming a real review quoting those strings after line 5 passes.

---

## 6. Codebase Alignment

**Violations**:
- **None in `src/`**. Note for clarity: the review criteria reference `Backend` trait and `BackendErrorKind`, which belong to the external `lok` orchestrator binary rather than `gcm`'s Rust crate. In `gcm`, the Rust abstractions are the synchronous `Provider` trait (`src/provider/facade.rs`), `ProviderError` / `ErrorKind` (`src/provider/identity.rs`), and `GcmError` (`src/error.rs`) built on `ureq` per `docs/adrs/001-foundational-architecture-decisions.md` and `docs/adrs/002-library-boundary.md`. The spec correctly avoids modifying `gcm` Rust code or `lok` source.
- **Cross-runtime `.pi/` vs `.claude/` field drift**: Intentionally left out of scope and explicitly mitigated by AC15/AC22 (legacy `review_ollama` fallback read in `.claude/` phase docs) and tracked in Follow-ups. Verify `node .pi/scripts/check-schema-parity.mjs` stays unaffected (it parses only `.pi/`, and `review_ollama` is `# optional` there).

**Alignment**:
- Preserves `.lok/workflows/*.toml` conventions, `.claude/commands/task/phases/*.md` state schemas, and `.pi/AGENTS.md` handoff expectations (`lok` exit codes, pre-PR filenames, synthesis verdicts).

---

## 7. Blind Spots

1. **`synthesis` `when` expression if kept**: Existing `spec-review.toml:177` and `pre-pr-validation.toml:202` use `when = "steps.gemini_review.success or steps.ollama_review.success or steps.claude_fallback.success"`. When external reviewers pass, `claude_fallback` is skipped (`undefined` in MiniJinja). If `gemini_review` or `codex_review` fails at the `lok` step level, or if the expression order ever evaluates `steps.claude_fallback.success` while skipped, MiniJinja errors and `lok` treats expression errors as `true`. If `synthesis` keeps a `when` clause, guard `steps.claude_fallback` with `is defined` (or drive it from `review_status` + `claude_fallback is defined`).
2. **`git diff main...HEAD` when `main` is missing or unborn**: In worktrees or repos without a local `main` branch (or if `origin/main` is used), `git diff main...HEAD` exits non-zero. Keeping `mkdir -p .lok/local/review-input` and checking `git diff main...HEAD` behavior (or failing clearly if the diff cannot be generated) avoids creating an empty `.diff` file silently.
3. **Duplicate `validate-review-output.md` unused prompt**: `.lok/prompts/validate-review-output.md` still references `APPROVE, APPROVE_WITH_SUGGESTIONS, NEEDS_REVISION` and is not referenced by the workflows (which inline `[steps.validate]` prompts). Not blocking, worth a quick check or follow-up note if anything reads it.

---

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The third revision resolves every blocking architectural and correctness defect from the earlier reviews (`design-review.toml` skipped fallback, `REVIEW_FAILED:` prefix vs substring, read-only `opencode --standalone` permission lockdown, `shell_escape` data passing, deterministic `NO_REVIEWS_AVAILABLE` + `verdict_gate`, AC18 structural checks, AC24 headings, and Pi handoff compatibility). Only narrow clarifications around `write_reviews` reason precedence, `review_status` `min_deps_success`, workflow stderr paths, `retries` on `REVIEW_FAILED`, and Evaluation #1's multiline regex remain.

---

## 9. Actionable Feedback

1. **[High] Clarify AC9 failure-reason precedence**: State explicitly that when `steps.X.output` already starts with `REVIEW_FAILED:` (emitted by the shell guard for `exit 1`, `exit 124`, `<100` chars, or help/usage), the write step preserves that guard line even when `[steps.validate]` marks the step unsuccessful (`success == false`), rather than replacing it with `validator rejected output` or `step failed`.
2. **[High] Add `min_deps_success = 0` and `continue_on_error = true` to `review_status` (AC8)**: Ensure `review_status` still runs when either or both external reviewer steps fail validation, and ensure `claude_fallback` depends on `review_status` (and guard any `steps.claude_fallback` check in `synthesis`'s `when` with `is defined`).
3. **[High] Specify deterministic per-slug, per-reviewer stderr paths for the 3 workflows (AC9)**: e.g., `.lok/local/review-input/<slug>-<reviewer>.stderr` (cleaned in `health_check`) so `write_reviews`/`write_reports` can actually attach the stderr excerpt when the validator rejects output or the guard did not run.
4. **[Medium] Fix Evaluation #1 multiline `--dir` regex and Evaluation #3 `.lok/lok.toml` assertions**: Replace `opencode[^\n]*--dir\b` with a multiline check (or `rg -n -- '--dir\b' ... | grep -v 'lok run'`), and assert `--auto` and `[backends.ollama]` `enabled = false` in Evaluation #3 for `.lok/lok.toml`.
5. **[Medium] Verify `[steps.validate]` + `retries = 3` interaction on `REVIEW_FAILED` / timeout (Evaluation #6)**: Confirm whether `retries = 3` causes a 240s child timeout inside a `5m` step to hit the outer `5m` `lok` timeout on retry #2; adjust `retries` or step timeout if needed so AC7 holds across retries.
6. **[Low] Tighten Sub-task 5, Sub-task 6, and Evaluation #11**: Note temp-file capture before writing headers in `implement.md` (instead of direct `-o`), remove `GEMINI_FALLBACK_MODEL` / `gemini-3.1` prose in `.claude/commands/design-doc/review.md`, document the new override history action in `orchestrate.md` / `workflow-state.yaml`, and add inline-gate failure/branch checks (0 usable reports, `codex_validated: false`, development-task inputs) and Edge Case 2 (`Usage:` after line 5) to the Evaluation table.
