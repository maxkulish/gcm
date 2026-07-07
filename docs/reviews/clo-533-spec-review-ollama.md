# Spec Review: clo-533

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-07-07
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is **clear and self-contained**. It correctly identifies that Phase 1 (CLO-531) already implements local conflict resolution, and Phase 2 extends it to remote MR/PR orchestration. The scope is well-bounded: a "thin fetch-then-invoke-core wrapper" that reuses the existing `src/resolve/` engine.

**Strengths:**
- Explicitly references the Phase 1 design (`docs/designs/clo-531-gcm-resolve.md`) and plan
- Clear problem framing: "developer can point `gcm resolve` at a GitHub PR or GitLab MR"
- Explicitly defines what Phase 2 does NOT do (no new resolver logic)

**Minor gap:** The Linear description mentions "thin fetch-then-invoke-core wrapper" but the spec could clarify that this is about *invocation* orchestration, not about adding new resolution capabilities. This is mostly implicit but could be made more explicit.

## 2. Acceptance Criteria Review

**Strong:**
- **AC1-AC3** (CLI surface, host auto-detection, missing CLI tool): Well-defined with specific verification methods. The mutual exclusivity check for `--pr`/`--mr` is clear.
- **AC4-AC7** (Scratch isolation, merge orchestration, resolution branch naming, default local-only): Clear, testable, and measurable.
- **AC8** (Dry-run purity): Excellent coverage of what must not happen.
- **AC11** (Phase-1 core unchanged): Clear constraint with specific verification that no new logic is added under `src/resolve/` except `src/resolve/remote/`.

**Gaps:**
1. **AC2 verification is incomplete**: The verification method `cargo test resolve_remote::parse_github_url -- --exact` tests URL parsing, but the spec also requires "A bare numeric id is resolved against the current repo's `origin` remote host." This second case needs a separate test for `resolve_remote::host_from_origin_remote`.

2. **AC9 missing error handling test**: `--remote-push` and `--remote-comment` can fail (network errors, permission errors, closed MR/PR). The spec mentions EC6/EC7 for these, but AC9's verification doesn't cover failure paths.

3. **AC10 escalation report shape**: The spec says "the final report lists the unresolved files" but doesn't specify the JSON shape for `RemoteReport`. The existing `ResolveReport` struct doesn't have a `remote` field yet.

4. **Missing AC for cleanup**: No acceptance criterion explicitly requires that the scratch repo is cleaned up on all exit paths (success, error, user abort). AC4 mentions `TempDir` but doesn't test the cleanup invariant.

5. **AC12 partially incomplete**: `cargo clippy -- -D warnings` passes for the *new* code, but integration tests (`tests/resolve_remote.rs`) aren't included in `cargo test resolve:: -- --exact`.

## 3. Constraints Check

**Aligned with codebase:**
- "No native GitHub/GitLab API client crate" - matches ADR-001 pattern of shelling out to tools on PATH (consistent with how `mergiraf` and `git` are used)
- "Authentication is inherited from the user's existing `gh`/`glab` login state" - consistent with the codebase's approach of not storing credentials
- Error handling via `GcmError` variants - consistent with `src/error.rs` patterns

**Concerns:**
1. **Missing constraint about timeout handling**: The spec doesn't discuss timeout management for `gh pr checkout` or `glab mr checkout` operations. The existing `http.rs` has `TIMEOUT_SECS` for provider calls, but shell-out operations have no timeout discussed.

2. **No constraint about network isolation in tests**: ST6 mentions "Real network calls are never made in tests" but there's no constraint about how `gh`/`glab` CLI availability is faked in unit tests vs integration tests.

3. **Missing `GcmError` variants for remote operations**: The spec mentions `GcmError::RemoteHost(String)` and `GcmError::RemoteCliMissing` but these don't exist in `src/error.rs` yet. This is expected for a new feature, but the constraint section should note that these additions are required.

4. **Implicit constraint about git CLI version**: The spec uses `gh pr checkout --branch <name>` but doesn't specify minimum `gh` CLI version or document fallback for older versions.

## 4. Decomposition Quality

**Well-scoped:**
- ST1 (CLI flags), ST2 (host detection), ST3 (temp-clone/merge), ST4 (publish), ST5 (report wiring) are each scoped appropriately
- ST6 (integration tests) is correctly sized as L (largest estimate)

**Issues:**
1. **ST3 underestimates complexity**: "Fetch source/target branches, checkout base, merge, invoke engine" is actually multiple distinct operations. The estimate is M but might need decomposition into:
   - ST3a: Create scratch clone
   - ST3b: Fetch branches via `gh`/`glab`
   - ST3c: Create resolution branch and merge
   - ST3d: Invoke `run_resolve_in_repo`
   
   Each of these has error paths that need handling.

2. **Missing sub-task for error type extensions**: Adding `GcmError::RemoteHost` and `GcmError::RemoteCliMissing` should be an explicit sub-task (or part of ST2).

3. **Dependency ordering is incomplete**: ST5 depends on ST4 for the `RemoteReport` fields (pushed, commented). ST2 should explicitly note it depends on `GcmError` extensions.

4. **ST6 mentions fixture scripts but not how they're invoked**: The verification method should clarify whether fixtures are:
   - Shell scripts on PATH (for integration tests)
   - Mocks in unit tests
   - Both

## 5. Evaluation Coverage

**Covered:**
- Scenarios 1-5 (URL parsing, host detection, missing CLI tools)
- Scenarios 6-8 (scratch isolation, branch naming, dry-run)
- Scenarios 10-12 (push, comment, escalation)

**Gaps:**
1. **Missing scenario for EC3 (clean merge)**: No test scenario where the merge produces no conflicts. The spec says "The report status is `resolved` or `noop`" but there's no evaluation row for this path.

2. **Missing scenario for partial resolution**: What happens if Phase 1 resolves some files but escalates others? This is mentioned in AC10 but not explicitly tested in the evaluation table.

3. **No test for concurrent scratch repo collision**: If two `gcm resolve --pr 123` invocations run simultaneously, the deterministic path `gcm-resolve-github-123` might collide. The spec says "deterministic or random path" but doesn't specify which or test this case.

4. **Missing test for `--dry-run` with `--remote-push`/`--remote-comment`**: AC8 says dry-run prevents "no clone, no merge, no branch, no provider write, no remote mutation" but there's no test that verifies `--dry-run --remote-push` is correctly rejected or silently ignored.

## 6. Codebase Alignment

**Violations:**
1. **ST5 proposes `run_resolve_in_repo(repo, args)` but current signature is `run_resolve(args: &Cli)`**: The refactoring is necessary but not trivial. The existing `resolve::run_resolve` assumes it can discover the repo via `Repo::discover()`. The spec should note that this refactoring is needed.

2. **Error handling pattern inconsistency**: The spec mentions `GcmError::RemoteHost(String)` but existing error variants in `src/error.rs` use structured fields (e.g., `ResolutionEscalated { path, reason }`). Consider:
   ```rust
   RemoteHost { host: String, reason: String }
   RemoteCliMissing { cli: String, install_hint: String }
   ```
   This follows the existing pattern better than `RemoteHost(String)`.

3. **Test file location mismatch**: The spec mentions `tests/resolve_remote.rs` but the existing integration tests use the pattern `tests/<name>_integration.rs` or `tests/<name>.rs` directly. The file `tests/resolve_integration.rs` already exists.

**Alignment:**
- The `tempfile::TempDir` pattern matches existing usage in `tests/resolve_integration.rs` and `src/git.rs`
- The JSON envelope extension follows the existing `ResolveReport` pattern in `src/resolve/report.rs`
- The CLI extension pattern (adding `--mr`/`--pr` to existing `Commands::Resolve`) matches how `Commands::Resolve` already accepts optional flags

## 7. Blind Spots

1. **Scratch repo cleanup on SIGINT/SIGTERM**: `TempDir` cleans up on Drop, but what happens if the user Ctrl+C during a merge operation? The process may leave partial state in the scratch repo. The spec doesn't discuss signal handling.

2. **Network timeout for `gh`/`glab` operations**: Unlike HTTP calls with `TIMEOUT_SECS`, shell-out CLI operations have no timeout. A hung `gh pr checkout` could block indefinitely.

3. **Git index/working-tree state during scratch operations**: The spec says "user's checked-out branch is never touched" but doesn't discuss what happens if the user modifies the source repo while Phase 2 is running. The scratch repo is a snapshot, but large repos might take time to clone.

4. **`gh`/`glab` authentication failures mid-operation**: EC4 mentions auth failure but not the recovery path. If `gh pr checkout` fails after clone but before merge, does the scratch repo get cleaned up?

5. **Provider call budget for remote conflicts**: The Phase 1 engine has `diff_budget` for provider calls. When resolving a remote MR/PR, should the budget be different? The spec doesn't discuss this.

6. **Progress/report output during long operations**: Cloning large repos, merging, and LLM resolution all take time. The spec doesn't discuss progress indication (stderr messages) or JSON streaming.

7. **Resolution branch name collisions**: If `gcm-resolve-github-123` already exists (from a prior run), what happens? The spec doesn't discuss branch management beyond "never the MR/PR source branch."

8. **`--json` output shape for remote operations**: The existing `ResolveReport` struct needs extension. The spec mentions `remote: RemoteReport` block but doesn't define the struct.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is well-structured, correctly scopes Phase 2 as orchestration over Phase 1's core, and follows established codebase patterns. The acceptance criteria are mostly testable, and the decomposition is reasonable. However, several gaps need addressing before implementation:

1. **Required fixes** (should be addressed):
   - Add missing `GcmError` variants with structured fields (not `RemoteHost(String)`)
   - Define the `RemoteReport` struct explicitly
   - Add an AC for scratch repo cleanup on error paths
   - Add evaluation scenario for clean merge (no conflicts)

2. **Clarifications needed**:
   - Specify whether branch name is deterministic or random (and collision handling)
   - Clarify ST3 scope or decompose it
   - Document `gh`/`glab` minimum versions or fallback behavior

3. **Minor enhancements**:
   - Add timeout consideration for CLI operations
   - Consider progress indication for long operations
   - Clarify test fixture approach (PATH manipulation vs mocks)

## 9. Actionable Feedback

**P1 - Must address before implementation:**

1. **Define `RemoteReport` struct explicitly in §3 or §6**: Add concrete type definition:
   ```rust
   pub struct RemoteReport {
       pub host: Host,           // GitHub | GitLab
       pub number: u64,          // PR/MR number
       pub base_branch: String,
       pub source_branch: String,
       pub resolution_branch: String,
       pub pushed: bool,
       pub commented: bool,
   }
   ```

2. **Add error variants with structured fields**: Replace `GcmError::RemoteHost(String)` with:
   ```rust
   RemoteHost { host: String, reason: String }
   RemoteCliMissing { cli: String, install_hint: String }
   ```

3. **Add AC for cleanup invariant**: 
   > AC13 - Scratch repo cleanup: On any exit path (success, error, user abort), the scratch directory is removed. No residual `gcm-*` temp directories remain after execution.
   > Verification: `cargo test resolve_remote::scratch_cleanup_on_error -- --exact`

4. **Add evaluation scenario for clean merge**: Add row for "Source branch merges cleanly into base" with expected behavior "status: resolved or noop, no LLM call, no conflicts in resolution branch."

**P2 - Should address for completeness:**

5. **Decompose ST3 or clarify scope**: Either:
   - Split into ST3a (scratch clone), ST3b (branch fetch), ST3c (merge orchestration)
   - Or add explicit error-handling notes for each sub-operation

6. **Specify branch name collision handling**: Add to EC or constraints:
   > EC11 - Resolution branch already exists: If `gcm-resolve-github-123` exists from a prior run, the new run either overwrites (checkout -B) or fails with guidance to delete or specify a different branch. Default behavior should be documented.

7. **Add `--dry-run` with remote flags test**: Add evaluation row for `gcm resolve --pr <url> --remote-push --dry-run` expected behavior.

**P3 - Consider for future refinement:**

8. **Document minimum CLI versions**: Add to constraints if `gh pr checkout --branch` requires specific version.

9. **Consider timeout for shell-out operations**: The `exec_command` utility could wrap with a timeout.

10. **Define `run_resolve_in_repo` signature**: Make ST5's refactoring explicit:
   ```rust
   pub fn run_resolve_in_repo(repo: &Repo, args: &Cli) -> Result<ResolveReport, GcmError>
   ```
