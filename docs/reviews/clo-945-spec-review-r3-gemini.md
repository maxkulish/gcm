# Spec Review (Round 3): clo-945

**Reviewer**: Gemini (google-vertex/gemini-4-hydrogen#high)
**Reviewed spec sha256**: 1fdf4ba2d04b
**Date**: 2026-11-19

---

## 1. Problem Statement Assessment

The problem statement (`docs/specs/2026-11-19-clo-945-review-gates.md:7-55`) is clear, well-evidenced, and matches the Linear description for CLO-945. It documents 21 concrete defects with verified CLI behavior (`opencode v2.3.2`, `codex-cli 0.170.0`, `lok 20261107.0.2`), exact file/line citations, the Pi compatibility boundary, and the maintainer decisions replacing Ollama and AI Studio (`google/`) with Codex (`gpt-6.2-sol`, `model_reasoning_effort='"high"'`) and Gemini (`google-vertex/gemini-4-hydrogen#high` via `opencode`).

One minor inaccuracy in defect 19 / AC24: `design-review.toml:181-204` (`claude_fallback`) currently uses `## 7. Verdict` and `## 8. Actionable Feedback`, not `## Priority Actions` (only `spec-review.toml:166` has `## Priority Actions` in its fallback prompt).

## 2. Acceptance Criteria Review

**Strong**:
- **AC1-AC5, AC10-AC13, AC15, AC19-AC22**: Concrete, measurable, and directly testable against the target files and runtime behavior, including `--standalone` + `OPENCODE_CONFIG_CONTENT` read-only enforcement (AC20), `shell_escape` data passing (AC21), and two-way Pi handoff compatibility (AC22).
- **AC6-AC9, AC18, AC25**: Cleanly separate CLI/guard failure detection (AC6), post-validation single-source usability checks via `review_status` and `fallback_status` (AC8), phase-specific structural checks for reviewer reports (AC18) and synthesis reports (AC25), and atomic file replacement plus `verdict_gate` non-zero exit propagation (AC9).
- **AC23**: Deterministically handles `main` / `origin/main` branch diff generation into `.lok/local/review-input/<slug>-branch.diff` and named input failure precedence (`REVIEW_INPUT_FAILED:`).

**Gaps**:
- **AC23 + AC26 (`pre-pr-validation.toml:168` fallback diff access)**: AC26 runs `claude_fallback` with `--tools "Read,Grep,Glob" --strict-mcp-config` (no `Bash` tool), and AC23 replaces `3. Run: git diff main...HEAD (to see all changes)` in the assembled `.lok/prompts/pre-pr-validation-prompt.md`. However, the inline `claude_fallback` prompt in `.lok/workflows/pre-pr-validation.toml:168` says `3. The git diff: run \`git diff main...HEAD\`` - a tool-less Claude fallback cannot run that command unless AC23/Sub-task 4 also replaces line 168 with the `.lok/local/review-input/<slug>-branch.diff` path.
- **AC8 (`min_deps_success = 0` on `fallback_status` and `review_status`; `synthesis` `when` in `design-review.toml`)**:
  - `fallback_status` depends on `claude_fallback`, which is skipped whenever `usable != 0`. Without `min_deps_success = 0` (and `continue_on_error = true` / `exit 0`), `lok` can skip `fallback_status` when `claude_fallback` is skipped, causing downstream references to `steps.fallback_status.output` to fail or mis-evaluate. `review_status` should likewise explicitly specify `min_deps_success = 0`.
  - `.lok/workflows/design-review.toml:209-213` currently has no `when` on `synthesis`. AC8 should require the `synthesis` `when` expression across all three workflows so `design-review.toml` skips the Claude synthesis call when `usable=0` and `fallback=failed`.
- **AC7 + AC26 (`claude_fallback` as a shell step)**: Now that `claude_fallback` is a shell step using the AC6 guard, the spec does not state: (a) its child timeout vs lok step timeout (`.lok/workflows/design-review.toml:144` currently has `timeout = "2m"` and `retries = 2`, leaving <=60s for Claude under AC7's 60s margin), (b) whether `claude_fallback` sets `retries = 0`, and (c) unsetting `CLAUDECODE` (`env -u CLAUDECODE` as `.lok/workflows/spec-review.toml:99` did) when `lok` is invoked from inside an active Claude Code session (as `.claude/commands/task/phases/spec.md:44` and `.claude/commands/design-doc/review.md:226` do).
- **AC9 / AC10 / AC16 (`implement.md` output capture and headers)**: `.claude/commands/task/phases/implement.md:74,82` currently passes `-o docs/reviews/clo-XX-codex-validation.md` directly to `codex exec` and redirects `opencode` directly to `docs/reviews/clo-XX-gemini-validation.md`. AC16 specifies `mktemp` for stderr only, leaving ambiguity about whether the inline gate captures outputs to temp files first, prepends the AC10 `**Reviewer**` header (and AC9 failure layout on failure), and atomically replaces the target files.
- **AC14 (overloaded criterion and unnamed history action)**: AC14 combines 6 distinct requirements across 3 workflows, `phases/spec.md`, `phases/design.md`, and `implement.md` into one >2,000-character bullet, and does not name the distinct history action string required when the inline gate overrides at 0 usable reports.

## 3. Constraints Check

**Aligned**:
- Preserving the workflow topology with minimal structural additions (`review_status`, `fallback_status`, `verdict_gate`, `continue_on_error = true` on `synthesis`, shell-step `claude_fallback`).
- POSIX `sh` syntax (`RC=0; ... || RC=$?`, `.` sourcing), `{% if steps.X is defined %}` template guards (matching `.pi/lessons/lok-workflow-templates.md` L1), `shell_escape` data passing, and read-only enforcement for all three reviewers.
- Keeping `.pi/`, historical `docs/reviews/`, `docs/status/`, `gcm` Rust source, and `lok` source out of scope.

**Concerns**:
- **Missing `Must` constraint on `env -u CLAUDECODE` for shell-invoked `claude -p`**: Existing `spec-review.toml:99` explicitly unsets `CLAUDECODE` when spawning an agent CLI from inside a Claude Code session. Without this constraint (or verified proof that `claude -p` ignores `CLAUDECODE`), converting `claude_fallback` from `backend = "claude"` to a shell step can fail when invoked from `/task:orchestrate` in Claude Code.
- **AC20 vs Evaluation #3 placement of `OPENCODE_CONFIG_CONTENT` and flags**: AC20 says `OPENCODE_CONFIG_CONTENT` has one definition in `.lok/lib/review-guard.sh`, while Evaluation #3 expects `OPENCODE_CONFIG_CONTENT`, `--agent plan`, and `--standalone` to match in all 3 workflow `.toml` files. Clarify the split (e.g., workflows invoke `opencode run ... --agent plan --auto --standalone` with `OPENCODE_CONFIG_CONTENT="$GEMINI_PERMISSION_CONFIG"` while `review-guard.sh` defines `GEMINI_PERMISSION_CONFIG` and performs the guard/validation checks, or update Evaluation #3 to check `review-guard.sh` directly).

## 4. Decomposition Quality

**Well-scoped**:
- Sub-task 1 (`.lok/lib/review-guard.sh` + stub proof) cleanly isolates the shared POSIX guard, AC18/AC25 structural checks, and AC20 config before wiring the workflows.
- Dependency ordering (`1` -> `{2, 3, 4, 5}` -> `6`) is clear and parallelizable.

**Issues**:
- **Sub-tasks 2, 3, 4 (`claude_fallback` prompt assembly via AC21)**: Because `claude_fallback` becomes a shell step, Sub-tasks 2, 3, and 4 must also assemble `claude_fallback`'s prompt using `shell_escape` for `arg.1`..`arg.5` (writing the prompt file under `.lok/local/review-runs/<slug>/` per AC26) instead of interpolating `{{ arg.* }}` inside shell strings, and Sub-task 4 must replace `run \`git diff main...HEAD\`` at `pre-pr-validation.toml:168` with the `.lok/local/review-input/<slug>-branch.diff` file.
- **Sub-task 5 (`implement.md`)**: Should explicitly include capturing Codex (`-o`) and Gemini stdout to per-run temp files, prepending AC10 `**Reviewer**` headers (or AC9 failure reports), and atomically moving them into `docs/reviews/`.
- **Sub-task 6 (`orchestrate.md` and `workflow-state.yaml`)**: Omits adding the distinct AC14 0-review override history action to `.claude/commands/task/orchestrate.md:98-135` and `.claude/templates/workflow-state.yaml:109-143`, and should explicitly note removing `GEMINI_FALLBACK_MODEL` / `gemini-3.1-pro-preview` / `OLLAMA_*` prose throughout `.claude/commands/design-doc/review.md` (not just the reference tables).
- **Sizing note**: Sub-tasks 2-4 (especially Sub-task 4, `pre-pr-validation.toml`) combine reviewer migration, status steps, fallback shell conversion, synthesis/writer gates, and diff-input handling, so they are likely >2 hours each even with Sub-task 1 completed first.

## 5. Evaluation Coverage

**Covered**:
- Strong static `rg` checks (#1-#3), stub-CLI failure and combination tests (#4-#8h), live runs (#9-#12, #15, #18), TOML/lok validation (#13), direct AC18/AC25 helper tests (#14), heredoc/argument injection tests (#16, #19), Pi two-way handoff (#17), diff-prep failure (#20), timeout across retries (#21), and read-only Claude fallback probe (#22).

**Gaps**:
- **Inline gate failure and task-type branches missing**: Evaluation #11 only tests `implement.md` on the happy path of a `specification` task, and #20 tests diff-prep failure (`pause`). There is no evaluation row for `implement.md` at 0 usable reports (`override`/`pause` only, `codex_validated: false`, distinct history action), 1 usable report with Codex failed (`codex_validated: false`), or a `development` task (`phases.design.design_doc` + `phases.plan.plan_file`).
- **Edge Case 2 not in the test table**: Line 166 ("A real review that quotes `Usage:` in a code block or `Unrecognized flag` in prose after its first 5 non-empty lines is not flagged") is listed under "Edge cases to verify" without a numbered test in the Evaluation table.
- **Evaluation #3 `.lok/lok.toml` assertions incomplete**: Evaluation #3 checks only `google-vertex/gemini-4-hydrogen#high` for `.lok/lok.toml`, omitting `--auto` in `[backends.gemini]` `args` (AC3) and `enabled = false` in `[backends.ollama]` (AC5).
- **Evaluation #14 fixture accuracy**: `docs/reviews/clo-945-spec-review-synthesis.md` ends with `## Verdict` and has no `## Priority Actions`, so it requires reshaping to test `synthesis-review`; `docs/reviews/clo-942-spec-review-synthesis.md` and `docs/reviews/clo-533-validation-synthesis.md` already exist in the prompt formats and can be tested directly alongside negative cases.

## 6. Codebase Alignment

**Violations**:
- **None in `src/`**. Note that the review criteria mention the async `Backend` trait and `BackendErrorKind`, which belong to the external `lok` orchestrator rather than `gcm`. In `gcm`, the synchronous `Provider` trait (`src/provider/facade.rs:46`), `ProviderError` / `ErrorKind` (`src/provider/identity.rs:16,74`), and `GcmError` (`src/error.rs:10`) use blocking `ureq` per `docs/adrs/002-library-boundary.md`, and this spec properly leaves all Rust code in `src/` and `lok` untouched.
- **`.pi/` schema parity**: Leaving `.pi/` untouched does not break `node .pi/scripts/check-schema-parity.mjs` because `review_ollama` is optional (`# optional` in `.pi/orchestrator/phases/{spec,design}.md`) and absent from `PHASE_CONFIG` in `.pi/extensions/orchestrate/index.ts`. AC15 and AC22 preserve backward-compatible reads on resume.

**Alignment**:
- Correctly adapts to `lok`'s template/shell constraints (`lok/src/template/context.rs`, `lok/src/workflow.rs`, `.pi/lessons/lok-workflow-templates.md` L1, `.gitignore`'s `.lok/local/`) without modifying `lok`.

## 7. Blind Spots

1. **`CLAUDECODE` env inheritance in the new `claude_fallback` shell step**: Because `claude_fallback` moves from `backend = "claude"` to a shell step calling `claude -p`, running `lok` from inside a Claude Code session (`.claude/commands/task/phases/spec.md`, `design-doc/review.md`) inherits `CLAUDECODE`, which can block nested `claude` execution unless `env -u CLAUDECODE` is used (as `spec-review.toml:99` did for `ollama launch codex`).
2. **Pre-PR `claude_fallback` diff access after `--tools "Read,Grep,Glob"`**: `pre-pr-validation.toml:168` still instructs `claude_fallback` to run `git diff main...HEAD`, which will fail without a shell tool.
3. **Skipped `claude_fallback` causing skipped `fallback_status`**: Without `min_deps_success = 0` on `fallback_status`, `lok` will skip `fallback_status` whenever external reviewers succeed and `claude_fallback` is skipped.
4. **In-flight `claude_fallback` timeout/retries under AC26**: `design-review.toml:144` has `timeout = "2m"` and `retries = 2` on `claude_fallback`; AC7's child-timeout margin and `retries = 0` rule should explicitly cover `claude_fallback` now that it is a guarded shell step.

## 8. Verdict

**NEEDS_REVISION**

The fourth revision resolves the structural, security, data-passing, and rerun-safety gaps across the workflow pipelines, but converting `claude_fallback` to a read-only shell step (AC26) and introducing `fallback_status` (AC8) leaves four concrete runtime bugs/gaps if implemented as written (`pre-pr-validation.toml:168` requiring `git diff` without a shell tool, `fallback_status` lacking `min_deps_success = 0`, `claude_fallback` missing `env -u CLAUDECODE` and timeout/retry settings, and `implement.md` lacking temp-file output capture/headers and named override history action).

## 9. Actionable Feedback

1. **[High] Fix `pre-pr-validation.toml:168`, prompt assembly, and `CLAUDECODE` for shell-step `claude_fallback` (AC21, AC23, AC26)**:
   - Update AC23 and Sub-task 4 so `pre-pr-validation.toml:168` replaces `run \`git diff main...HEAD\`` with reading `.lok/local/review-input/<slug>-branch.diff`.
   - Require Sub-tasks 2, 3, and 4 to assemble `claude_fallback`'s prompt via `shell_escape` (writing under `.lok/local/review-runs/<slug>/`) and run `env -u CLAUDECODE` (or verify `claude -p` under `CLAUDECODE=1`).
   - Specify `claude_fallback`'s child timeout (at least 60s under its lok step timeout, raising `design-review.toml:144` from `2m` if needed) and `retries = 0` per AC7.
2. **[High] Require `min_deps_success = 0` on `review_status` / `fallback_status` and `when` on `design-review` `synthesis` (AC8)**:
   - Explicitly require `min_deps_success = 0` and `continue_on_error = true` (plus `exit 0`) on both `review_status` and `fallback_status` so `fallback_status` runs when `claude_fallback` is skipped.
   - Apply the same `review_status`/`fallback_status`-based `when` condition to `synthesis` in all three workflows (including `design-review.toml`).
3. **[Medium] Specify temp-file output capture and headers for `implement.md` (AC9, AC10, AC16, Sub-task 5)**:
   - Update AC16 and Sub-task 5 so `implement.md` writes both `codex exec -o` and `opencode` outputs to per-run temp files, prepends the AC10 `**Reviewer**` header (or AC9 failure layout on failure), and atomically replaces `docs/reviews/clo-XX-{codex,gemini}-validation.md`.
4. **[Medium] Name the AC14 override history action and align Sub-task 6 & Evaluation #3**:
   - Name the distinct history action for a 0-review inline override in AC14 and include adding it to `.claude/commands/task/orchestrate.md` and `.claude/templates/workflow-state.yaml` in Sub-task 6 (and split AC14 into focused sub-bullets or separate ACs for readability).
   - Align AC20, Sub-task 1, and Evaluation #3 on whether `--agent plan`, `--standalone`, and `OPENCODE_CONFIG_CONTENT` live in the workflow files or inside `.lok/lib/review-guard.sh`, and add `--auto` and `enabled = false` assertions for `.lok/lok.toml` to Evaluation #3.
5. **[Low] Close remaining Evaluation gaps**:
   - Add test coverage for `implement.md` failure/branch paths (0 usable reports -> `override`/`pause` + `codex_validated: false`, 1 usable report with Codex failed, and `development`-task inputs) and add Edge Case 2 (`Usage:` / `Unrecognized flag` after line 5) to the Evaluation table.
   - Note in defect 19 / AC24 that only `spec-review.toml`'s fallback prompt had `## Priority Actions` (`design-review.toml:181-204` already uses `## 7. Verdict` and `## 8. Actionable Feedback`).
