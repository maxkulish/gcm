# Spec Review: clo-555

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-07-12
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is exceptionally clear, complete, and highly detailed. It maps perfectly to the Linear task description, citing precise file:line evidence for the four critical adjacent defects:
1. **Unsafe prompt parser** (`src/ui.rs:44-48`, `src/ui.rs:89-100`) allowing anything except `n`/`e` to accept (including empty inputs and EOF).
2. **Pre-confirmation mutation** destroying manual resolutions.
3. **Uncentralized staging** missing marker-free or mergiraf-resolved files.
4. **Remote Partial hazard** committing raw conflict markers and pushing them unconditionally.

It provides a concrete design-aligned fix ("Yes to all = stage and finish; No to any = byte-exact restore") and successfully identifies and excludes out-of-scope work (such as the rebase loop under CLO-554).

## 2. Acceptance Criteria Review
**Strong**: 
* **Precision and Measurability**: Criteria (AC1-AC12) are fully objective and highly technical, detailing explicit Git index states, index files, and postcondition-driven outcome classifications.
* **Robust Edge-Case Coverage**: AC2 guarantees byte-level comparison (preserving manual partial edits) for restoration, AC7 defines a non-destructive hook failure path, and AC8 mitigates the remote double-commit risk while gating pushes on resolved/noop states.
* **Unified UI Semantics**: AC3 closes the unsafe prompt parser gap by unifying both the commit flow (`confirm`) and resolve flow prompts into a single, bounded, safe parser where Enter/EOF defaults to Abort.

**Gaps**:
* **Commit-Flow Behavioral Shift**: AC3 specifies that Enter/EOF now aborts for the main `gcm commit` flow's message confirmation prompt. In previous releases, hitting Enter accepted. This is a significant breaking behavioral change for standard users of `gcm commit` who are accustomed to hitting Enter to commit. While a deliberate choice for unified safety, this should be highlighted prominently in the release/migration notes.
* **Escalation under `--yes`**: AC5 details that under `--yes`, any escalated file skips the finish phase and stages progress. However, it should be explicitly noted that `--yes` automatically confirms all non-escalated files without a prompt, proceeding directly to phase C (Apply) for those files.

## 3. Constraints Check
**Aligned**:
* **Byte-level snapshotting** prevents data loss or UTF-8 conversion errors on binary/raw conflicted files.
* **Three-phase execution model** (propose, confirm, apply) ensures a transactional execution.
* **GIT_LITERAL_PATHSPECS=1** matches existing codebase hygiene to prevent unintended wildcards in paths containing glob metacharacters.
* **Process inheritance** is aligned with the GPG/SSH pinentry requirements of the existing opinionated `commit_signed` implementation.

**Concerns**:
* **`GcmError::leaves_staged` Integration**: The specification fails to mandate updating `leaves_staged(&self) -> bool` in `src/error.rs` to return `true` for the new `GcmError::FinishFailed` variant. If this is omitted, standard error execution paths might trigger a destructive index cleanup, violating AC7's directive to keep the staged state intact.
* **GPG Pinentry Failures**: Forcing `commit.gpgsign=true` on continues means that if a user's pinentry agent fails or is unconfigured in non-interactive terminal contexts, the command will fail. The manual continue output must be clearly displayed so the user can easily recover.

## 4. Decomposition Quality
**Well-scoped**:
* The sub-tasks (ST1-ST8) are logical, modular, and sized perfectly for 1-2 hour execution windows.
* The dependency path is well-defined (ST1 and ST2 are independent, ST4 acts as the orchestrator of ST1+ST2, ST5 integrates ST3+ST4).

**Issues**:
* No major issues. One minor recommendation is to explicitly group the implementation of the new error `GcmError::FinishFailed` in ST3 alongside the `leaves_staged` integration update.

## 5. Evaluation Coverage
**Covered**:
* The test plan covers all 12 ACs with precise unit and integration test definitions.
* Probe-gating is appropriately planned for CI runs where GPG keys are unavailable.
* Specific verification of CRLF, no-final-newline, and pre-existing staged index states are addressed.

**Gaps**:
* **SIGINT/Ctrl-C Interruption**: The test matrix does not address physical user interruptions. If a user presses `Ctrl-C` during Phase B (Confirm), the process terminates instantly. Because Phase C (Apply) hasn't executed, the working tree will be left with the zdiff3/mergiraf mutated state and the raw pre-run bytes won't be restored. Since standard OS signals are not caught by default, this remains a known limitation of the transaction and should be documented.

## 6. Codebase Alignment
**Violations**:
* None. The spec rigorously mimics existing architecture, error patterns, and command-line execution patterns.

**Alignment**:
* Respects the existing single-subcommand execution flow and matches the `Provider` trait boundary.
* Reuses GCM's subprocess stdin/stderr inheritance patterns from `commit_signed` rather than the standard captured `run_git`.
* Extends `ResolveReport` while preserving JSON structure compatibility and Schema Version (`v=1`).

## 7. Blind Spots
* **Signal Handling (Ctrl-C Data Loss)**: As noted in Section 5, hard-interrupts (`SIGINT`) mid-run bypass the restore-snapshot sequence, leaving the mutated working tree intact and potentially causing minor loss of pre-run manual resolutions.
* **Git Version Compatibility**: Some older versions of Git do not support `-c commit.gpgsign=true` alongside `rebase --continue` or `cherry-pick --continue` in the same format. The integration test matrix should run on different platforms to ensure compatibility.

## 8. Verdict
```
APPROVE_WITH_SUGGESTIONS
```

## 9. Actionable Feedback
1. **Mandate `leaves_staged` update**: Explicitly require updating `leaves_staged` in `src/error.rs` to return `true` for `GcmError::FinishFailed` to ensure staged files are preserved when a hook or finish step fails.
2. **Warn on `gcm commit` breaking change**: Add a prominent warning in both the README update (ST8) and release notes regarding the behavioral change to `gcm commit` (where Enter/empty now aborts rather than commits).
3. **Clarify `--yes` + escalation semantics**: Explicitly document in the spec that `--yes` skips the confirmation phase by auto-accepting all non-escalated files while preserving progress staging for any escalated files.
4. **Document SIGINT limitation**: Add a brief note to the safety guarantees in `README.md` clarifying that hard-aborting (`Ctrl-C`) mid-resolve will leave the working tree mutated with zdiff3 markers and will not trigger the automatic byte-for-byte snapshot restoration.
