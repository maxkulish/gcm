# Spec Review: clo-554

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-07-28
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is clear, complete, and highly accurate. It outlines how the current single-stop rebase resolution design forces the user to rerun `gcm resolve` N times for an N-commit rebase sequence. 

It correctly identifies that the key risk of a multi-commit resolution loop is LLM provider spend. Since proposals are generated before user confirmation, a naive loop could burn tokens on subsequent commits without the user's consent. The proposed solution addresses this through:
- An interactive, per-round "gate" that prompts the user before generating proposals for the next stop.
- A hard configurable cap (`--max-rounds`, defaulting to 10) to bound costs during unattended `--yes` runs.
- Robust transaction/rollback boundaries using the established `WorkingTreeSnapshot` contract.

The description is completely aligned with the Linear issue context.

## 2. Acceptance Criteria Review

**Strong**:
- The criteria are highly specific, measurable, and map directly to test cases.
- **AC2 (Round gate)** specifies exactly what metadata (commit SHA and conflicted file count) must be displayed to the user before prompting, ensuring an informed decision.
- **AC4 (No mid-loop data loss)** guarantees transactional isolation by preserving committed states of rounds `1..k-1` while cleanly rolling back round `k` if rejected.
- **AC5 (Per-round visibility)** protects machine output separation by writing round banners and statistics strictly to `stderr` and preserving a single JSON block on `stdout` when `--json` is active.
- **AC6 (JSON report)** maintains strict backward compatibility by omitting the new `loop` key in single-round/legacy runs.

**Gaps**:
- **AC for `--dry-run` and `--no-finish`**: While mentioned in Section 3 (Constraints) and Section 5 (Edge cases), the single-stop behavior of these flags should be explicitly defined as Acceptance Criteria.
- **No-progress termination behavior**: The exit code and JSON status for a no-progress termination are mentioned in Section 3 and Section 5, but not explicitly pinned as an AC.

## 3. Constraints Check

**Aligned**:
- Putting the loop driver in `run_resolve` and keeping `run_resolve_in_repo` as a single-round function is an excellent separation of concerns. This ensures remote resolution pipelines (which operate on a scratch repo merge and never produce `StoppedOnNextConflict`) remain untouched.
- Keying the loop continuation on `FinishResult::StoppedOnConflict` in the return report is highly robust, allowing both rebases and cherry-picks to loop transparently.
- Mirroring the `max_rounds` default in both the serde default and the hand-written `Default` implementation for `ConflictConfig` addresses a known config drift vulnerability.

**Concerns**:
- None. The constraints are well-thought-out, defensive, and align beautifully with the codebase patterns.

## 4. Decomposition Quality

**Well-scoped**:
- The 7 sub-tasks are highly modular and properly isolated.
- Sub-tasks 1, 2, 3, and 4 are independent and parallelizable.
- The dependency chain is correctly identified: the loop driver (5) depends on 1–4, the terminal output (6) depends on 3 and 5, and the integration tests (7) depend on 5 and 6.
- Each sub-task is appropriately scoped to under 2 hours of development effort.

**Issues**:
- None.

## 5. Evaluation Coverage

**Covered**:
- The test suite is exceptionally thorough. The 12 test cases cover multi-commit rebases, interactive rejections, gate denials, caps, validation rejections, escalations, non-looping merges, serialization, prompting mechanics, and git head helpers.

**Gaps**:
- There is no explicit test case in the evaluation table verifying that `--no-finish` and `--dry-run` terminate after exactly one round and omit the `loop` report object (although they are noted in Section 5 under Edge cases).
- There is no test case in the table that simulates or forces the "no-progress" guard to fire.

## 6. Codebase Alignment

**Violations**:
- None.

**Alignment**:
- **Error Handling**: Custom exit/validation errors map cleanly to the established `GcmError::Config` and `GcmError::FinishFailed` paradigms.
- **Configuration Precedence**: Merging command line arguments into `ConflictConfig` mirrors the established pattern in `resolve_conflict_config` and `ConflictCli`.
- **UI & Prompting**: Reusing `parse_choice` and `PROMPT_ATTEMPTS` inside the new yes/no helper in `src/ui.rs` preserves prompt behavior and keeps parsing centralized.

## 7. Blind Spots

- **External state changes during interactive prompts**: While the user is parked at the interactive `[y/N]` round gate, they could switch to another terminal and manually resolve, abort, or continue the rebase. The "no-progress" guard and the existing "external edit check" will catch this when the loop driver re-evaluates the repository state at the beginning of the next round, but it is worth noting that the head SHA must be re-read immediately after the gate is accepted.
- **Commit signing credentials timeout**: If the user has SSH or GPG commit signing enabled, git may prompt for passphrases/pinentry or require a hardware token touch (e.g., YubiKey) during `git rebase --continue` on *each* round. This is expected behavior, but should be noted as an operational characteristic for users running loops interactively.

## 8. Verdict

`APPROVE_WITH_SUGGESTIONS`

## 9. Actionable Feedback

1. **Add Acceptance Criteria for `--dry-run` and `--no-finish`**:
   Add `AC9` and `AC10` in Section 2 specifying that both `--dry-run` and `--no-finish` execute exactly one round, perform no looping, and omit the `loop` block from the JSON report.
2. **Elevate "no-progress" guard to Acceptance Criteria**:
   Explicitly define the no-progress termination behavior in Section 2, ensuring that when the loop terminates due to no progress, it exits cleanly (exit 0) with `loop.terminal == "no_progress"` and prints a clear warning diagnostic.
3. **Expand the test suite (Section 5) to verify `--no-finish` / `--dry-run` and No-Progress**:
   - Add a test verifying that calling `gcm resolve` with `--no-finish` runs exactly one round and does not loop.
   - Add a test that forces a no-progress condition (e.g. by mocking a successful finish that returns `StoppedOnNextConflict` but doesn't change `REBASE_HEAD`) and asserts that the loop terminates with `no_progress`.
4. **Re-evaluate non-TTY/non-interactive state transitioning**:
   Add a constraint clarifying that any non-TTY execution without `--yes` or `--no-input` is caught and rejected by the existing `needs_terminal_but_absent` check before the first round even begins.
