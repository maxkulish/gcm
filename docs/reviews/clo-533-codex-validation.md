# Pre-PR validation: clo-533

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

HIGH - Default remote resolve loses the only local result. The spec says default mode is local-only and prints the scratch path plus resolution branch, while cleanup is also required. Current code commits inside a `TempDir` scratch repo, does not push by default, does not include a scratch path in `RemoteReport`, and `main.rs` only prints branch/base/source/status. Once `run_resolve_remote_opt` returns, the temp repo is dropped, so default users have no usable branch.

HIGH - Acceptance tests are misleadingly named but mostly dry-run only. Tests do not exercise the non-dry-run merge/commit/push/comment behavior they claim to verify. This leaves AC5, AC6, AC9, AC10, AC13, and AC14 effectively unproven.

MEDIUM - URL parsing can target the wrong repo for common GitLab shapes and malformed URLs. `parse_url` searches for `pull` / `merge_requests` anywhere, then blindly assigns `owner = path_segments[0]` and `repo = path_segments[1]`. GitLab subgroup URLs like `group/subgroup/repo/-/merge_requests/42` would clone `group/subgroup`, not `group/subgroup/repo`; malformed GitHub URLs like `/acme/pull/42` can parse as repo `pull`.

MEDIUM - Resolution branch checkout errors are ignored. `checkout -B <resolution_branch> <base>` is assigned to `_`, so a failure can continue into merge/commit/reporting on the wrong branch. This violates the AC5/AC6 orchestration guarantee and weakens error handling.

LOW - The worktree has an uncommitted `docs/status/clo-533-workflow.yaml` update claiming rework completion. It is not part of `main...HEAD`; either commit it intentionally or remove it before PR.

## Missing Items

AC7 is not implemented correctly: default local-only output is not durable and does not print a scratch repo path.

AC2 is incomplete for robust full URL parsing, especially GitLab subgroup namespaces and malformed path rejection.

AC5, AC6, AC9, AC10, AC13, and AC14 are not adequately acceptance-tested despite test names matching the spec.

## Recommendations

Resolve the AC7/AC13 contract first: either retain/export the scratch repo for default local-only success, or make default produce a durable local artifact somewhere outside `TempDir`; then include that path in human and JSON output.

Replace the dry-run placeholder tests with real fake-CLI integration tests that assert the resolution branch commit contents, push invocation, comment invocation, clean merge behavior, partial escalation markers, and cleanup-on-error.

Tighten URL parsing to validate exact GitHub/GitLab PR/MR path shapes and preserve GitLab namespace paths correctly.

Change the ignored checkout to `?` and add a regression test for branch creation failure.

Verification: `cargo fmt --check` passed. `git diff --check main...HEAD` fails on doc trailing whitespace. Read-only sandbox, did not run `cargo test` or `clippy`.
