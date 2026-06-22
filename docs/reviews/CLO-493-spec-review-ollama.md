# Spec Review: CLO-493

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-22
**Pipeline**: lok spec-review

---

Now I have a comprehensive understanding of the codebase, existing patterns, and the specification. Let me provide my review.

---

## 1. Problem Statement Assessment

**Strong**: The problem statement is clear, self-contained, and well-motivated. It correctly identifies three distinct but related requirements:
1. Machine-readable output (`--json`) for CI/agent orchestration
2. Non-interactive safety (`--yes`/`--no-input`/`--plan-only`) for unattended commits
3. Structured logging with proper stream separation (stderr for logs, stdout for JSON)

The statement accurately references the Linear task description and cross-references ADR-001 decisions (#10 for non-TTY behavior, #6 for message contracts) and PRD FR-49/50/51. The distinction between `--dry-run` (preview but save cache) and `--plan-only` (pure preview, no mutation) is clearly articulated.

**Minor Gap**: The problem statement mentions `GCM_LOG_LEVEL` as a new env var but doesn't explicitly reference the existing `GCM_DEBUG` variable that's already implemented in `src/debug.rs`. The relationship should be made explicit (though AC-10 does address this).

## 2. Acceptance Criteria Review

**Strong**:
- **AC-1 through AC-8** are specific, measurable, and include concrete verification commands using `jq`. The verifiable examples (e.g., `jq -e '.status == "plan" and .plan.groups'`) provide clear test predicates.
- **AC-5** (non-TTY fail is explicit) correctly references ADR-001 #10 and provides an actionable error code (`NonInteractive`).
- **AC-6** (fallback visibility) ties directly to CLO-492's fallback work with a specific JSON schema requirement.
- **AC-9 and AC-10** (logging controls) address a real operational need and provide clear verification.

**Gaps**:

1. **Missing AC for exit codes**: The spec doesn't explicitly define exit codes for different JSON output states. Should `status: "error"` always exit 1? What about `status: "noop"` — exit 0 or a special code? This is critical for CI scripting. *Suggested addition*: AC-11 defining exit codes (0 for success/committed/plan/noop, non-zero for errors).

2. **AC-8 ambiguous overlap with AC-1**: AC-8 says `--dry-run --json` returns `status: "plan"` with `plan_mode != null`. But AC-1 says `--plan-only --json` returns `status: "plan"`. Are these the same output schema? The spec should clarify whether `--dry-run` and `--plan-only` produce identical JSON or differ in some field (e.g., `mode: "dry_run"` vs `mode: "plan_only"`).

3. **Missing AC for schema versioning**: The JSON schema should include a version field for forward compatibility. Without `schema_version` or `v: 1`, future changes will break consumers.

4. **Missing AC for `--json` with existing `--all`**: AC-7 covers `--all --yes --json` for the committed case, but there's no AC for `--all --plan-only --json` or `--all --dry-run --json` to verify the single-commit preview schema.

5. **Missing AC for concurrent invocations**: If two `gcm --json` processes run on the same repo, what happens? Cache contention isn't addressed. (This may be out of scope but worth noting.)

## 3. Constraints Check

**Aligned**:
- "Must not add async runtime" aligns with ADR-001 Decision 2 (blocking HTTP client).
- "Must not change plan schema" aligns with the existing `Plan` struct.
- "Keep warnings on stderr, stdout clean for JSON" aligns with the stated goal.
- Gate on "will the index be mutated" (not just `--dry-run`) is correctly forward-looking to `--plan-only`.

**Concerns**:

1. **Missing constraint about stdout pollution**: The spec says logs go to stderr, but doesn't explicitly forbid debug output from third-party libraries (e.g., `ureq`'s internal logging) from polluting stdout. Need a constraint like: "Before any JSON is written, validate that stdout is uncontaminated by third-party library output."

2. **No constraint about JSON schema stability**: The spec should mandate that `status`, `error.code`, `commit.sha`, and `plan.groups` are stable field names. A constraint like "JSON field names MUST NOT change in v1.x" would help.

3. **Missing escalation for schema conflicts**: If the existing `Plan` struct (used for cache persistence) conflicts with the new JSON output schema, the spec doesn't say whether to evolve `Plan` or create a separate `JsonOutput` struct. The ST1 text suggests "one serializable envelope" but should be explicit.

## 4. Decomposition Quality

**Well-scoped**:
- **ST1 (JSON output model)** is appropriately sized and creates a new `output.rs` module.
- **ST2 (CLI flags)** is small and focused.
- **ST3 (execution routing)** correctly identifies `run/execute` as the touch point.
- **ST4 (logging)** is isolated to `debug.rs` and provider modules.

**Issues**:

1. **ST3 may be larger than estimated**: "Route every execution path through typed output" touches `main.rs`, `ui.rs`, and every branch in `execute()`. The existing code has 6+ distinct paths (grouping commit, single-commit, fallback, merge, dry-run, plan-only). Each needs careful JSON mapping. This could be 4+ hours, not 2.

2. **Missing ST for error schema mapping**: The existing `GcmError` enum has ~8 variants. Mapping each to a JSON `error.code` string needs a dedicated sub-task or clear spec. Currently implicit in ST1 but should be explicit.

3. **Missing dependency on CLO-492**: The spec references `status: "fallback"` and `fallback.reason`, but doesn't explicitly state that this work depends on CLO-492's `PlanError` Display being machine-readable. The `BuildError::Fallback(reason)` already carries the `PlanError` Display, but the JSON schema should specify `error.code` values (e.g., `"NonInteractive"`, `"ProviderError"`, `"GitError"`).

4. **ST5 (acceptance) may need mock infrastructure expansion**: The existing mock harness (`PLAN_FILE`, `GCM_GROQ_BASE_URL`) may not easily support JSON output verification. The spec should clarify whether new mock infrastructure is needed.

## 5. Evaluation Coverage

**Covered**:
- Scenarios 1-10 map directly to AC-1 through AC-10.
- Test approaches (unit vs integration) are realistic.
- Edge cases section is thorough and covers `--plan-only`/`--dry-run` overlap, clean repo, partial staging, etc.

**Gaps**:

1. **Missing test for JSON output with binary/non-UTF8 paths**: The spec doesn't address how file paths with non-ASCII characters appear in JSON output. `git status --porcelain=v1 -z` handles this, but JSON serialization needs verification.

2. **Missing test for large outputs**: What happens if `plan.groups` contains 100+ files? Is there a size limit? Should the spec mention streaming vs buffered output?

3. **Missing test for concurrent cache writes**: If `gcm --json` writes to the cache while another `gcm --json` reads, what guarantees exist? (May be out of scope for v1 but worth noting.)

4. **Missing test for interrupted writes**: If `gcm --yes --json` commits but the process is killed before writing JSON to stdout, what state is the repo in? (AC-4 mentions `status: "committed"` but doesn't address this edge case.)

## 6. Codebase Alignment

**Violations**:

1. **`output.rs` creates a new module, but existing output is ad-hoc**: The current code uses `println!` and `eprintln!` scattered throughout. ST1 proposes a "single emitter," but there's no existing `output.rs` or `Output` trait. The spec should clarify whether this is:
   - A new struct `JsonOutput` that replaces `println!` calls
   - A trait `Output` with `Human` and `Json` impls
   - An enum `OutputMode` passed to `execute()`

2. **`debug_log!` macro doesn't support levels**: The existing `src/debug.rs` only has `enabled()` (boolean) and `debug_log!`. Adding `GCM_LOG_LEVEL=warn|info|debug|trace` requires substantial changes to this module. ST4 should clarify whether this is a refactor or a new implementation.

3. **Missing integration with `ui::confirm`**: The `ui::confirm` function currently returns `Decision` and prints to stdout. In JSON mode, should it:
   - Skip the prompt entirely (already done for `--yes`)?
   - Return the decision as part of JSON output?
   The spec says `--yes --json` returns `status: "committed"`, but doesn't explain how `Decision::Commit` maps to JSON. This is implicit but should be explicit.

**Alignment**:

1. **ADR-001 #10 is correctly implemented**: `src/ui.rs:needs_terminal_but_absent` already checks for non-TTY and returns `true` when `!auto_yes && !dry_run`. This aligns with AC-5.

2. **`Plan` struct is already `Serialize`**: The existing `src/plan.rs` derives `Serialize`, so `serde_json::to_string(&plan)` works directly. Good alignment.

3. **Error handling patterns match**: The `GcmError` enum and `ProviderError` struct follow the established pattern. Adding `NonInteractive` as a JSON-error code is straightforward.

4. **Cache invalidation already considers provider/model**: `src/cache.rs` folds `model` into the fingerprint, so `--plan-only` doesn't need special cache handling (it just doesn't advance).

## 7. Blind Spots

1. **What happens to `display_groups` output in JSON mode?** The existing `display_groups` function (lines 248-262 in `main.rs`) prints human-readable group info. In JSON mode, should this be:
   - Suppressed entirely?
   - Included in `plan.groups` (already)?
   - Printed to stderr?
   The spec assumes `plan.groups` is sufficient but doesn't explicitly address this.

2. **Editor path (`e` option) in JSON mode**: `ui::confirm` has an `e` option to edit the message. In JSON mode with `--yes`, this is moot. But what about JSON mode *without* `--yes` (non-interactive)? The spec says this errors (AC-5), but should there be a JSON schema for "edit required, please provide edited message via stdin/arg"?

3. **Merge state detection in JSON**: `main.rs:84` checks `repo.is_merging()` to bypass grouping. What does JSON output look like for a merge-commit? The spec mentions `--all --json` but not merge-specific output.

4. **GPG signing failures**: `GcmError::CommitFailed` includes signing failures. The spec should clarify whether `status: "error"` with `error.code: "CommitFailed"` is sufficient, or if more detail is needed.

5. **Provider error mapping**: `ProviderError` has 10 variants (MissingKey, RateLimit, Auth, etc.). The spec should define which map to `error.code: "ProviderError"` and which get specific codes (e.g., `error.code: "RateLimit"`).

6. **Logging to stderr may conflict with progress output**: If the user wants progress indicators during a long LLM call, where do they go in JSON mode? The spec is silent on this. (May be out of scope for v1.)

7. **`--reset` flag in JSON mode**: Does `--reset` produce any JSON output, or is it silent? The spec doesn't mention `--reset` at all.

8. **Cache hit scenario**: If a cached plan exists and `--plan-only --json` is run, should the output indicate `cached: true` or just return the plan? This affects reproducibility for CI.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is well-structured, addresses a clear need, and aligns with the existing codebase patterns. The acceptance criteria are mostly testable, and the decomposition is reasonable. However, there are several gaps that should be addressed before implementation:

1. **Critical**: Define exit codes for each JSON status
2. **Critical**: Clarify `--dry-run` vs `--plan-only` JSON schema differences
3. **Important**: Add schema versioning field
4. **Important**: Specify `error.code` values for all error types
5. **Important**: Clarify `output.rs` architecture (struct vs trait vs enum)
6. **Minor**: Address `display_groups` suppression in JSON mode
7. **Minor**: Document `--reset` behavior in JSON mode

## 9. Actionable Feedback

**Priority 1 (Must address before implementation)**:

1. **Add AC-11 defining exit codes**:
   ```
   AC-11: JSON mode exits 0 for status: "plan"|"committed"|"noop" and non-zero for status: "error"|"fallback" (fallback commits successfully, so exit 0; errors exit 1).
   ```

2. **Clarify `--dry-run` vs `--plan-only` in JSON schema**:
   - Either: They produce identical output (`status: "plan"`)
   - Or: They differ by a `mode` field (`mode: "dry_run"` vs `mode: "plan_only"`)
   - Recommend: Same output, since both are no-mutation previews

3. **Add `v` or `schema_version` field to JSON output**:
   ```json
   {"v": 1, "status": "plan", ...}
   ```

4. **Define `error.code` enum values** (create a `ErrorCode` enum or const list):
   ```
   "NonInteractive", "NotARepo", "Git", "Provider", "UnmergedConflicts", "CommitFailed", "Editor", "EmptyMessage"
   ```

**Priority 2 (Should address for completeness)**:

5. **Specify `output.rs` architecture** in ST1:
   - Recommend: An `Output` enum with `Human` and `Json` variants, passed to `execute()`
   - Alternative: A `JsonOutput` struct with `emit()` method, called at each exit point

6. **Document `display_groups` behavior in JSON mode**:
   - Recommend: In JSON mode, `display_groups` is suppressed; all group info is in `plan.groups`

7. **Add constraint about stdout purity**:
   ```
   Constraint: In JSON mode, no output except the final JSON object may be written to stdout. Third-party libraries (ureq, etc.) must not log to stdout.
   ```

8. **Add AC for `--all --plan-only --json`**:
   ```
   AC-12: gcm --all --plan-only --json on a dirty repo returns a single-commit preview with status: "plan" and mode: "single".
   ```

**Priority 3 (Nice to have)**:

9. **Document `--reset` in JSON mode**: Is it silent, or does it return `status: "reset"`?

10. **Consider `cached: true` field for cache hits**: Helpful for CI reproducibility debugging.

11. **Add ST for `ErrorCode` mapping**: Explicit mapping from `GcmError` variants to `error.code` strings.
