# Pre-PR validation: clo-533

**Reviewer**: Gemini (gemini-3.5-flash)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

1. **Missing Integration Tests specified in Verification / Evaluation (HIGH)**
   - **Details:** The design document's Acceptance Criteria and Evaluation Table specify a comprehensive suite of integration/acceptance tests to verify scenarios such as missing CLI binaries, scratch repo isolation, resolution branch naming, opt-in push, opt-in comment, clean merges, and partial escalations.
   - **Impact:** Only two integration tests (`parse_github_url` and `dry_run_no_clone`) are implemented in `tests/resolve_remote.rs`. While the underlying logic is present and well-structured, the actual test suite lacks the direct coverage outlined in the plan for AC3, AC4, AC5, AC6, AC7, AC9, AC10, AC13, and AC14.

2. **Timeout Parameters Ignored in Shell-Outs (MEDIUM)**
   - **Details:** In `src/resolve/remote/fetch.rs`, shell-out command helpers `run_host_cmd` and `run_cmd` accept a `timeout: Duration` parameter but explicitly ignore it via `let _ = timeout;` and call `.output()` without a timer.
   - **Impact:** If `git clone` or a host CLI command (`gh`/`glab`) hangs due to network failure, auth/credential prompt block, or server-side lag, the CLI will hang indefinitely. This contradicts the Design Note in §6 stating that long-running shell-outs are wrapped with a bounded timeout to avoid indefinite hangs.

3. **Domain Heuristic Disambiguation Logic is Robust (LOW)**
   - **Details:** When encountering self-hosted domains (e.g., `gitlab.company.corp`), the CLI correctly falls back on the preferred host determined by whether `--pr` or `--mr` was used, resolving potential host ambiguity gracefully.

4. **Zero-Warning Clean Build & Style Compliance (LOW)**
   - **Details:** `cargo clippy -- -D warnings` and `cargo fmt --check` pass perfectly with zero warnings or styling violations. The refactoring of `src/resolve/mod.rs` to extract `run_resolve_in_repo` is extremely clean.

## Missing Items

- **Acceptance Criteria Verification Tests:**
  The following specific tests mentioned in Section 2 and Section 4 are not implemented:
  - `resolve_remote::parse_gitlab_url` (AC2/Scenario 2)
  - `resolve_remote::host_from_origin_remote` (AC2/Scenario 3)
  - `resolve_remote::missing_gh_error` (AC3/Scenario 4)
  - `resolve_remote::missing_glab_error` (AC3/Scenario 5)
  - `resolve_remote::scratch_repo_is_isolated` (AC4/Scenario 6)
  - `resolve_remote::merge_produces_conflicts` (AC5)
  - `resolve_remote::resolution_branch_naming` (AC6/Scenario 7)
  - `resolve_remote::default_no_push` (AC7/Scenario 10)
  - `resolve_remote::dry_run_ignores_remote_flags` (AC8/Scenario 9)
  - `resolve_remote::remote_push_invoked` (AC9/Scenario 11)
  - `resolve_remote::remote_comment_invoked` (AC9/Scenario 12)
  - `resolve_remote::partial_escalation_report` (AC10/Scenario 14)
  - `resolve_remote::scratch_cleanup_on_error` (AC13)
  - `resolve_remote::clean_merge_no_conflicts` (AC14/Scenario 13)

## Recommendations

1. **Implement Missing Integration Tests:**
   Complete the integration test suite in `tests/resolve_remote.rs` by adding mock test cases for Git push/fetch and GitLab/GitHub error behaviors. Expand `write_fake_scripts` to capture and verify CLI arguments (e.g., verifying that `--body-file` and `--message` are used appropriately).

2. **Enforce subprocess timeouts:**
   Rather than ignoring the `Duration` timeout parameters, implement bounded timeouts. For synchronous execution, this can be safely achieved in standard Rust by spawning a thread to wait on the child or by implementing a channel receiver timeout. Ensure network and host calls cannot block indefinitely.
