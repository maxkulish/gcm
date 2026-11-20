# Spec Review: clo-945

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Reviewed**: 2026-11-19
**Pipeline**: manual run of the fixed opencode command (lok spec-review not used, it is the subject of this task)

---

## 1. Problem Statement Assessment

The problem statement is clear, well-evidenced, and matches the Linear task description. It identifies five concrete root causes backed by verified CLI output (`opencode v2.3.2`, `codex-cli 0.170.0`) and exact file/line references, and explicitly documents the maintainer's decision to supersede `gpt-6.2-astra` with `gpt-6.2-sol` and use `google-vertex/gemini-4-hydrogen#high`.

**Minor inaccuracies to clean up**:
- **File count mismatch**: Line 5 states `Estimated scope: M (8 files, 5 sub-tasks)` (and Section 3 Escalate refers to "the 8 files listed in Section 4"), whereas Section 4 lists **10 files** (1 new `.lok/lib/review-guard.sh` + 9 modified files across `.lok/` and `.claude/`).
- **Incomplete health-check diagnosis**: Root cause 1 cites the review steps (`spec-review.toml:46-49`, `design-review.toml:41-50`, `pre-pr-validation.toml:107-116`) for invalid `opencode` flags, but the `health_check` steps (`spec-review.toml:24`, `design-review.toml:13`, `pre-pr-validation.toml:27`) also pass `--dangerously-skip-permissions`, causing `gemini=false` on every run (which causes `design-review.toml:32` to short-circuit immediately).

---

## 2. Acceptance Criteria Review

**Strong**:
- AC1–AC5, AC8, and AC9 are concrete, greppable, and directly address each failure mode from CLO-941/CLO-942 and the Linear requirements.
- AC6 clearly specifies the three failure conditions (non-zero exit, `< 100` bytes, help/usage text) and requires `REVIEW_FAILED: <reviewer> <model>: <reason>` with the first 5 stderr lines.
- AC11 adds an end-to-end live proof on this spec.

**Gaps**:
- **AC6 vs. Edge Case 2 contradiction**: AC6 says `(it contains Unrecognized flag, or a line that is exactly USAGE or starts with Usage: or DESCRIPTION)`, while Section 5 Edge Cases narrows `USAGE`/`Usage:`/`DESCRIPTION` to *"column 0 within the first 5 non-empty lines of output only."* That scope boundary belongs in AC6 itself, and `Unrecognized flag` should likewise be anchored (e.g., within the first 5 non-empty lines or stderr) so a real review of a CLI tool like `gcm` quoting `"Unrecognized flag"` in prose is not falsely rejected.
- **AC3 / AC9 omit `.claude/` sibling files**:
  - AC3 checks `.lok/` and `.claude/commands/task/` for `google/` and API key env vars, omitting `.claude/commands/design-doc/review.md` (which Evaluation #1 does check, and which still references `gemini-3.1-pro-preview`, `gemini-2.5-pro`, `OLLAMA_MODEL`, `OLLAMA_TIMEOUT`, `--model ollama`, and `glm-5:cloud`).
  - AC9 omits `.claude/templates/workflow-state.yaml` (lines 45 and 121 still use `review_ollama` and `Gemini + Ollama`) and `.claude/templates/review-synthesis.md` (lines 3, 21, 36, 44 still reference `Ollama` and `glm-5:cloud`), even though `.claude/commands/design-doc/review.md:290` directs reviewers to use `review-synthesis.md`.
- **AC7 synthesis `**Reviewers**` header gap**: AC7 requires *"the `**Reviewers**` line of the synthesis files name the reviewer and the exact model string that ran"*, but `spec-review.toml:240` and `pre-pr-validation.toml:360-363` currently have no `**Reviewers**:` line at all (only `design-review.toml:313` has one). AC7 should explicitly state that `spec-review.toml` and `pre-pr-validation.toml` add this line (or scope it clearly), and clarify how AC7 applies to `implement.md` raw output files (which currently receive raw `codex exec -o` / `opencode run >` output without a `**Reviewer**:` header).
- **AC1/AC2 env overrides in `implement.md`**: Allowing literal `gpt-6.2-sol` and `google-vertex/gemini-4-hydrogen#high` in `.claude/commands/task/phases/implement.md` means `CODEX_MODEL` and `GEMINI_MODEL` overrides do not apply to the inline implement gate. If intentional, state why; otherwise use the same `${CODEX_MODEL:-...}` / `google-vertex/${GEMINI_MODEL:-...}` pattern.

---

## 3. Constraints Check

**Aligned**:
- Preserving the workflow topology (`health_check` -> parallel external reviewers with `[steps.validate]` -> `claude_fallback` -> `synthesis` -> write step), keeping the shell guard ahead of `[steps.validate]` (`on_error = "pass"`), and keeping `continue_on_error = true` + `exit 0` on `REVIEW_FAILED` all match the existing `.lok/workflows/*.toml` architecture.
- Keeping `.pi/orchestrator/` out of scope (tracked in Follow-ups), not touching historical `docs/reviews/` or `docs/status/` files, and not modifying `gcm` Rust code are sensible boundaries.

**Concerns**:
- **Missing constraint on `lok` conditional template guards (`.pi/lessons/lok-workflow-templates.md § L1`)**: `lok` aborts with a template error if a skipped step's `.output` or `.success` is referenced without `{% if steps.X is defined %}`. This must be a **Must** constraint for any `claude_fallback` references in `synthesis` and `write_reviews`.
- **Missing constraint on shell `set -e` / POSIX portability**: If `lok` runs shell steps with `set -e` or `/bin/sh`, `OUTPUT=$(cmd ...)` with a non-zero exit code will abort the step before the guard runs, and `source` will fail under POSIX `sh`. Capture exit codes safely (`RC=0; ... || RC=$?`) and use `. .lok/lib/review-guard.sh` (POSIX dot-source) if sourcing a helper.
- **File count in Escalate**: *"outside the 8 files listed in Section 4"* contradicts the 10 files actually listed in Section 4 (and will grow further if `.claude/templates/workflow-state.yaml` and `review-synthesis.md` are added).

---

## 4. Decomposition Quality

**Well-scoped**:
- Breaking the work into (1) shared guard + stub tests, (2) `spec-review.toml` + `design-review.toml`, (3) `pre-pr-validation.toml`, (4) `implement.md` + `lok.toml`, and (5) phase/command docs is logical, parallelizable across 2/3/4 once 1 is done, and well within ~2 hours per sub-task.

**Issues**:
- **Sub-task 2 (`design-review.toml`) misses critical pre-existing template bugs**:
  1. `design-review.toml:227-228` (`synthesis`) and `design-review.toml:305-309` (`write_reviews`) reference `{{ steps.claude_fallback.success }}` and `{{ steps.claude_fallback.output }}` **without** `{% if steps.claude_fallback is defined %}`. Previously `claude_fallback` always ran because both reviewers failed; once Gemini or Codex succeeds, `claude_fallback` will be skipped and `design-review.toml` will crash on those unguarded template variables (the exact CLO-799 failure in `.pi/lessons/lok-workflow-templates.md § L1`). Additionally, `FALLBACK_OUTPUT='{{ steps.claude_fallback.output }}'` breaks on single quotes in fallback output; it should use a heredoc like `pre-pr-validation.toml`.
  2. `design-review.toml:32-35` and `:94-97` check `steps.health_check.output` for `gemini=false` / `ollama=false` and emit `"REVIEW_FAILED: Gemini CLI unreachable ..."` (wrong format per AC6, and `ollama=false` must become `codex=false`). Also, `design-review.toml:79` omits `REVIEW_FAILED` from `[steps.validate]`'s `NOISE` list.
- **Sub-task 4 (`.lok/lok.toml`) conflicts with Evaluation #1**: Setting `[backends.ollama]` to `enabled = false` while leaving `model = "glm-5:cloud"` on `.lok/lok.toml:35` will fail Evaluation #1 (`rg` for `glm-5` in `.lok`). Either remove or replace `model = "glm-5:cloud"` in `[backends.ollama]`. Also note `[backends.gemini]` has `google/` on **both** line 28 (`model = ...`) and line 29 (`args = [...]`).
- **Sub-task 4 (`implement.md`) missing `codex -o` failure handling**: `implement.md:74` uses `codex exec -o docs/reviews/clo-XX-codex-validation.md`. When `codex` fails or exits non-zero, the `-o` file may be absent, empty, or stale from a prior run. Sub-task 4 needs to capture exit code + stderr (`2>/tmp/...`) and ensure the guard writes `REVIEW_FAILED: ...` into the target `.md` file on failure (and truncates/avoids stale files).
- **Sub-task 5 missing template files**: `.claude/templates/workflow-state.yaml` and `.claude/templates/review-synthesis.md` should be included in Sub-task 5. Also, `design-doc/review.md:19, 168` (`--model ollama` -> `--model codex`) and `:407-411` (`GEMINI_FALLBACK_MODEL`, `OLLAMA_MODEL`, `OLLAMA_TIMEOUT`, `gemini-3.1-pro-preview`) need explicit mention.

---

## 5. Evaluation Coverage

**Covered**:
- Good mix of static `rg` checks (#1–#3), deterministic stub-CLI failure tests (#4–#6), live review/config probes (#7–#9), and TOML syntax validation (#10).

**Gaps & Bugs in Evaluation Commands**:
- **Evaluation #1 false-positives on `lok run --dir .`**: The regex `--dir\b` matches `lok run ... --dir .` at `.claude/commands/task/phases/spec.md:50` and `.claude/commands/design-doc/review.md:229` (`lok run` has a valid `-d, --dir <DIR>` option, used by Evaluation #7 itself!). Narrow the pattern to flag `--dir` only in `opencode` invocations (e.g., `opencode[^\n]*--dir\b`).
- **Evaluation #1 misses stale model/env strings**: It checks `glm-5` and `gpt-5`, but does not check `gemini-3\.`, `gemini-2\.`, `OLLAMA_MODEL`, or `GEMINI_FALLBACK_MODEL`, so stale references in `design-doc/review.md` or `lok.toml` could slip past.
- **Evaluation #3 `rg -c` doesn't verify all four strings**: Using `A|B|C|D` only verifies that at least one of the four strings matches per file (and `.lok/lok.toml` doesn't even contain `gpt-6.2-sol` or `model_reasoning_effort`). Check each required pattern individually per target file.
- **Missing test for Edge Case 5 (`GEMINI_MODEL=gemini-4-hydrogen`)**: Evaluation #8 only tests `CODEX_MODEL` override, not `GEMINI_MODEL` without `#variant`.
- **Missing test for `design-review.toml` and `pre-pr-validation.toml` skip/fallback paths**: Edge cases mention one-fails / both-fail synthesis behavior, which should have concrete stub-CLI checks (especially to verify the `{% if steps.claude_fallback is defined %}` fix in `design-review.toml` when at least one reviewer succeeds).
- **Missing test for `[steps.validate]` interaction with `REVIEW_FAILED` + `retries`**: Verify whether `REVIEW_FAILED` (< 200 chars vs >= 200 chars) triggers `retries = 3` in `lok` or passes cleanly on the first attempt.

---

## 6. Codebase Alignment

**Alignment**:
- `Cargo.toml` and `src/` (`src/provider/mod.rs`, `src/provider/vertex.rs`, `src/provider/gemini.rs`) confirm `gcm` is a synchronous `ureq`-based Rust CLI and library; this spec correctly avoids touching Rust source, `gcm`'s `Provider`/`Vertex` implementations, or the locked library boundary (`docs/adrs/002-library-boundary.md`). (The prompt's mention of the async `Backend` trait and `BackendErrorKind` applies to `lok`, not `gcm`'s Rust crate.)
- Preserves the workflow structure and `docs/reviews/` naming conventions expected by `.claude/commands/task/phases/`.

**Violations / Friction**:
- **Violation of `.pi/lessons/lok-workflow-templates.md § L1` in `design-review.toml`**: Un-updated `design-review.toml` has unguarded `{{ steps.claude_fallback.success }}` / `.output` references that will crash `lok` once external reviewers start succeeding.
- **Cross-runtime schema drift (`.pi/` vs `.claude/`)**: `.pi/AGENTS.md` notes that `.claude/commands/task/` and `.pi/orchestrator/` mirror each other on `docs/status/clo-XX-workflow.yaml`. Renaming `review_ollama` -> `review_codex` in `.claude/commands/task/phases/{spec,design}.md` without updating `.claude/templates/workflow-state.yaml` (and noting `.pi/extensions/orchestrate/index.ts:133,153` in Follow-ups) leaves the two runtimes using different optional YAML field names. (It will not break `node .pi/scripts/check-schema-parity.mjs` because `review_ollama` is optional and not in `PHASE_CONFIG`, which is safe to defer if documented.)

---

## 7. Blind Spots

1. **False `REVIEW_FAILED` trigger when reviewing this spec (AC11!)**:
   `spec-review.toml:146`, `design-review.toml:149`, and `pre-pr-validation.toml:161` use `'REVIEW_FAILED' in steps.X.output` (unanchored substring match), and `spec-review.toml:191` says `"If all reviewers contain REVIEW_FAILED"`. When Gemini and Codex review **this spec** for AC11, their reviews will naturally quote `REVIEW_FAILED`, triggering `claude_fallback` (or even `NO_REVIEWS_AVAILABLE`) even though both reviews succeeded! Check whether `lok` supports line-start matching or a distinct sentinel/check, or ensure the synthesis prompt checks for a line *starting* with `REVIEW_FAILED:`.
2. **Broken `health_check` probe logic**:
   `timeout 15 opencode run "reply OK" ... | grep -qi "ok"` ignores `opencode`'s exit code and matches `"ok"` as a substring in words like `"token"`, `"invoke"`, or `"broken"`. An auth failure printing `"Error: invalid token"` sets `GEMINI_OK=true`. Conversely, a >15s Vertex cold start sets `GEMINI_OK=false` and causes `design-review.toml:32` to abort a 300s review step before even trying. Check exit status (`[ $? -eq 0 ]`) and word boundaries (`grep -qiw "ok"`), or increase/drop the `design-review.toml` short-circuit timeout.
3. **Multiline stderr breaking single-line `REVIEW_FAILED` contract**:
   `pre-pr-validation.toml:309` (`grep -q '^REVIEW_FAILED'`) and `validate-review-output.md` expect `REVIEW_FAILED: ...` on a single line. If the guard interpolates raw `head -5 "$ERR_FILE"` with embedded newlines, it remains single-line only on line 1 while spilling raw stderr onto lines 2–5. Flatten newlines (e.g., `tr '\n' ' '`) when formatting the reason.
4. **Separate shell steps cannot share a shell variable for headers (AC7)**:
   `gemini_review`/`codex_review` and `write_reviews`/`write_reports` execute in separate `lok` shell steps. AC7's *"derived from the same variable the step used"* works via the shared process environment variables (`CODEX_MODEL`, `GEMINI_MODEL`), so `write_reviews`/`write_reports` must evaluate the exact same `${CODEX_MODEL:-gpt-6.2-sol}` and `google-vertex/${GEMINI_MODEL:-gemini-4-hydrogen#high}` expressions (and if `GEMINI_MODEL` accidentally includes `google-vertex/`, decide whether to strip `${GEMINI_MODEL#google-vertex/}` or reject/document it).
5. **Dual `implement` phase files**:
   `.pi/orchestrator/phases/implement.md:218-227` invokes `lok workflow run pre-pr-validation` directly and documents `CODEX_MODEL=gpt-5.5`, `GEMINI_MODEL=gemini-3.5-flash`, `GEMINI_FALLBACK_MODEL=gemini-2.5-pro`. Because `pre-pr-validation.toml:9` literally points readers to `.pi/orchestrator/phases/implement.md`, changing `pre-pr-validation.toml` without either updating that comment or noting it in Follow-ups leaves a broken cross-reference.

---

## 8. Verdict

**NEEDS_REVISION**

The problem diagnosis and core direction are solid, but several concrete defects in the spec's details will cause implementation or evaluation failures (`design-review.toml` crashing on unguarded `claude_fallback` once reviewers succeed, Evaluation #1 false-failing on `lok run --dir .` and `.lok/lok.toml`'s `glm-5:cloud`, and `'REVIEW_FAILED' in ...` false-triggering when reviewing this spec in AC11).

---

## 9. Actionable Feedback

1. **[Critical] Guard `claude_fallback` in `.lok/workflows/design-review.toml` (Sub-task 2 + Must constraint)**:
   Wrap `{{ steps.claude_fallback.success }}` and `{{ steps.claude_fallback.output }}` in `synthesis` (lines 227–228) and `write_reviews` (lines 305–311) with `{% if steps.claude_fallback is defined %}...{% endif %}`, and replace `FALLBACK_OUTPUT='...'` with a heredoc (`cat <<'EOF'`), matching `spec-review.toml` and `pre-pr-validation.toml` (per `.pi/lessons/lok-workflow-templates.md § L1`).
2. **[Critical] Fix Evaluation #1 regex and `.lok/lok.toml` `[backends.ollama]`**:
   - Change `--dir\b` in Evaluation #1 to `opencode[^\n]*--dir\b` so `lok run ... --dir .` in `spec.md:50` and `design-doc/review.md:229` does not fail the check.
   - Add `gemini-3\.|gemini-2\.|OLLAMA_MODEL|GEMINI_FALLBACK_MODEL` to Evaluation #1, and update Sub-task 4 to remove/clear `model = "glm-5:cloud"` from `[backends.ollama]` in `.lok/lok.toml` (not just `enabled = false`).
3. **[High] Resolve the `REVIEW_FAILED` substring false-positive in `when` and `synthesis`**:
   Address how `spec-review.toml:146`, `design-review.toml:149`, `pre-pr-validation.toml:161`, and `synthesis` instructions handle real reviews (such as AC11 on this spec) that mention `REVIEW_FAILED` in prose—either anchor/prefix-check the failure sentinel (or use a distinct token / check line-start) so a real review quoting `REVIEW_FAILED` does not trigger `claude_fallback` or `NO_REVIEWS_AVAILABLE`.
4. **[High] Fix `health_check` reliability and `design-review.toml` short-circuit**:
   Require `opencode` exit code 0 + word-boundary `grep -qiw "ok"` (or equivalent) in `health_check`, fix `design-review.toml:32-35` and `:94-97` to check `codex=false` and emit the AC6 `REVIEW_FAILED: <reviewer> <model>: <reason>` format, and check `REVIEW_FAILED` in `design-review.toml`'s `[steps.validate]` `NOISE` list.
5. **[High] Reconcile AC6, Edge Case 2, and guard implementation details**:
   - Move the *"first 5 non-empty lines at column 0"* restriction into AC6 directly (and clarify where `Unrecognized flag` is matched).
   - Specify flattening stderr newlines in `review-guard.sh`, capturing exit codes safely under `set -e` (`RC=0; ... || RC=$?`), and using POSIX `.` instead of bash-only `source`.
   - In Sub-task 4 (`implement.md`), specify how `codex exec -o` failures/missing files are captured and overwritten with `REVIEW_FAILED: ...`.
6. **[Medium] Align file counts and missing `.claude/` files**:
   - Fix the scope header (`8 files` -> `10 files` or actual final count) and Section 3 Escalate count.
   - Add `.claude/templates/workflow-state.yaml` and `.claude/templates/review-synthesis.md` to AC9 and Sub-task 5, include `.claude/commands/design-doc/review.md` in AC3, and add `**Reviewers**:` headers to `spec-review.toml` and `pre-pr-validation.toml` synthesis files (or scope AC7).
   - Update `pre-pr-validation.toml:9`'s stale comment pointing to `.pi/orchestrator/phases/implement.md` (and note `.pi/extensions/orchestrate/index.ts` `review_ollama` in Follow-ups).
7. **[Medium] Strengthen Evaluation #3 and add missing edge-case tests**:
   - Change Evaluation #3 so it asserts every required string is present in each target file rather than a combined `rg -c` alternation.
   - Add explicit evaluation rows for `GEMINI_MODEL=gemini-4-hydrogen` (no `#variant`), `design-review.toml` / `pre-pr-validation.toml` success + fallback paths, and `[steps.validate]` behavior when `REVIEW_FAILED` is emitted with `retries = 3`.
