# Spec Review: clo-533

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-07-07
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement in the specification is exceptionally clear, self-contained, and completely aligned with the Linear task description. It correctly frames Phase 2 as a thin fetch-then-invoke orchestration layer over the Phase 1 core engine.

However, there is an unstated assumption regarding authentication. The specification assumes that the external CLI tools (`gh` and `glab`) are already authenticated and have correct read/write permissions for target repositories. If authentication is missing, the orchestration layer must fail gracefully with highly actionable hints for user setup.

---

## 2. Acceptance Criteria Review

**Strong**:
- **AC1 & AC8**: The specification establishes perfect isolation and dry-run boundaries, ensuring that no file mutation or clone occurs under `--dry-run`.
- **AC7**: Restricting remote actions by defaulting to local-only is an excellent safety guard.
- **AC12**: Code quality criteria are measurable and integrated with testing boundaries.

**Gaps**:
- **Clean Merge Handling**: If the source branch merges cleanly into the base branch, `resolve::run_resolve_in_repo` will abort with `NoConflictInProgress` or `NoConflicts` because it currently expects a conflict state to proceed. The criteria must define a custom path for conflict-free merges that exits cleanly as a `noop` or `resolved` status without throwing errors.
- **Authentication Mapping**: While AC9 details `--remote-push`, it does not specify how the scratch clone will authenticate with remote Git operations. Pushing over HTTPS inside a temporary repository typically requires configuring a credential helper (e.g., `git config credential.helper "!gh auth git-credential"`).

---

## 3. Constraints Check

**Aligned**:
- Treating `gh` and `glab` as external CLI tools on `PATH` perfectly mimics existing patterns in the codebase, such as the use of `mergiraf` and `git`.
- Reusing `resolve::run_resolve_in_repo` ensures global configuration variables, such as `.gcmignore` and `--secret-scan`, are honored seamlessly.

**Concerns**:
- **Error Types**: The specification proposes custom error handling patterns. To remain consistent with `src/error.rs`, we must extend the central `GcmError` enum with explicit orchestration variants (such as `GcmError::RemoteHost`) rather than using standard `anyhow` structures.

---

## 4. Decomposition Quality

**Well-scoped**:
- **ST1 & ST2**: The parser and CLI flags are highly independent and well-scoped.
- **ST6**: Writing fixture-based acceptance tests using script-based wrappers for `gh`/`glab` is the correct path to avoid real network operations during testing.

**Issues**:
- **ST3**: This step is too large and has a hidden dependency on the clean merge resolution mechanism. If ST3 relies directly on the existing `run_resolve_in_repo` as written in Phase 1, it will fail to handle conflict-free merges. ST3 must be decomposed to first adapt the local engine's entry point to support zero-conflict parameters.

---

## 5. Evaluation Coverage

**Covered**:
- The evaluation table maps perfectly to the 12 ACs, utilizing clear unit and integration-level assertions.

**Gaps**:
- **Self-Hosted Hosts**: The test coverage completely omits custom domains for self-hosted GitHub Enterprise or GitLab instances. Testing must cover parsing origin remote URLs with custom domains (e.g., `git@gitlab.company.corp:group/repo.git`).
- **No-Conflict Resolution**: No scenario covers a clean merge. The test plan must assert that a conflict-free remote resolve returns `noop` or `resolved` with a successful exit code.

---

## 6. Codebase Alignment

**Violations**:
- None. The specification successfully avoids introducing foreign error-handling types like `anyhow` or `BackendErrorKind`, instead planning to extend `GcmError` and utilize the `Provider` trait interface.

**Alignment**:
- The specification maintains the core `Provider` trait contract by keeping all remote orchestration code isolated to `src/resolve/remote/` as an outer wrapper.
- Standard JSON envelope structures (`ResolveReport`) are preserved and printed strictly on `stdout`, while human diagnostics go to `stderr`.

---

## 7. Blind Spots

- **Git Credential Helper Setup**: Inside the temporary repository clone, subsequent `git push` operations will fail over HTTPS unless the CLI credential helper is configured on the temporary repository config level.
- **Stderr Routing**: Host CLI stderr output from `gh`/`glab` could leak to the user's stdout and corrupt `--json` parser consumers unless carefully routed to `stderr`.
- **Custom SSH Keys**: If the origin remote uses SSH but the user has a custom SSH key setup, the temporary clone needs to preserve SSH agent connections.

---

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

---

## 9. Actionable Feedback

1. **Refactor Core for Clean Merges (Critical)**: Refactor `resolve::run_resolve_in_repo` so that when invoked by the remote orchestrator, it accepts a flag indicating that a clean merge (no conflicts) is a valid success state (returning `noop` or `resolved` instead of `NoConflictInProgress`).
2. **Credential Helper Injection (High)**: During scratch clone initialization in ST3, write the local Git configuration to hook into the parent host's credential helper (e.g., `git config credential.helper "!gh auth git-credential"`).
3. **Regex Support for Custom Hosts (Medium)**: In `src/resolve/remote/host.rs`, implement host parsing using pattern heuristics on the domain structure to identify GitHub vs GitLab, ensuring self-hosted instances on custom domains are supported.
4. **Command Stderr Isolation (Medium)**: Ensure all shelled-out commands (`gh`, `glab`, `git`) capture stdout/stderr separately, and forward their errors strictly to `stderr` so as not to break `--json` stdout output.
5. **Dry-Run Opt-Out (Low)**: Clearly specify in AC8 that under `--dry-run`, no temporary directory is created, and no external checkout tools are invoked. Heuristics must be mocked using the local checkout data.
