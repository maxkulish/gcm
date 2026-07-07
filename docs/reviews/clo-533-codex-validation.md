# Pre-PR validation: clo-533

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

**CRITICAL** - Remote resolutions are never staged or committed, so the result is lost or pushes the wrong tree.
The spec requires resolutions to land on `gcm-resolve-<host>-<number>` (spec line 26), but the remote flow only writes files via the Phase-1 engine and then optionally runs `git push` (remote/mod.rs:75, fetch.rs:205). There is no `git add`, no merge commit, and the `TempDir` is dropped at return (fetch.rs:22). Default mode discards the work; `--remote-push` pushes the branch ref, not the resolved tree.

**HIGH** - Self-hosted and unsupported hosts are handled incorrectly and can target the wrong public repo.
`RemoteRef` stores only `Host`, `owner`, `repo`, and `number`, then `format_origin_url` hardcodes `https://github.com/...` or `https://gitlab.com/...` (host.rs:46, fetch.rs:81). A self-hosted `gitlab.company.corp/acme/app` resolves to `gitlab.com/acme/app`. Also, `detect_host` accepts any unknown full URL as GitHub/GitLab when `--pr`/`--mr` supplies a preferred host (host.rs:197), violating the unsupported-host edge case.

**HIGH** - Clean merges still fail as `NoConflicts`.
After `git merge --no-ff --no-commit`, a clean merge still leaves a merge state with no unmerged paths. `run_resolve_in_repo(..., allow_no_conflict_state=true)` only treats no-conflict as success when there is no conflict state; if there is merge state and `unmerged_files()` is empty, it returns `GcmError::NoConflicts` (resolve/mod.rs:67, resolve/mod.rs:85). AC14 is not implemented.

**HIGH** - Default remote runs do not print or emit required remote metadata.
AC7 requires the scratch repo path and resolution branch by default (spec line 28). In non-JSON remote mode, `main.rs` prints nothing (main.rs:125). In JSON mode, `report.remote` is only populated inside the push/comment branch, so default remote runs omit `remote` entirely (remote/mod.rs:77).

**MEDIUM** - Dry-run full URLs still require a local git repo and host CLI.
`run_resolve_subcommand` calls `Repo::discover()` before deciding whether a remote dry-run can be parsed from a full URL (main.rs:82). Then `run_resolve_remote` checks `gh`/`glab` before the dry-run short-circuit (remote/mod.rs:34). That is stricter than the dry-run purity/preview contract.

**MEDIUM** - The documented bounded timeouts are not implemented.
The helpers accept `Duration` but discard it and call blocking `.output()` (fetch.rs:233, fetch.rs:262, publish.rs:98). This violates the spec's bounded shell-out constraint.

**MEDIUM** - Comment failure aborts the whole resolution, contrary to EC7.
`publish(...)?` propagates comment errors and fails the command (remote/mod.rs:77), while the spec says a closed/merged PR/MR comment failure should be surfaced without aborting the local resolution.

**LOW** - Acceptance coverage is far below the spec.
Only `parse_github_url` and `dry_run_no_clone` exist under `tests/resolve_remote.rs`, plus a few host unit tests. The named tests for GitLab integration, origin lookup, missing CLIs, scratch isolation, branch naming, default no-push, push/comment, partial escalation, cleanup, and clean merge are absent. `git diff --check main...HEAD` also fails on trailing whitespace in docs.

## Missing Items

AC4, AC5, AC6, AC7, AC9, AC10, AC13, and AC14 are not functionally covered. AC2 is only partially implemented and is wrong for self-hosted/unsupported hosts. AC8 is only partially implemented.

## Recommendations

1. Persist the remote result: after `run_resolve_in_repo`, stage the merged/resolved tree and create a commit on `gcm-resolve-<host>-<number>` before any push or cleanup.
2. Preserve the actual remote clone URL/domain in `RemoteRef`; reject unsupported full URLs instead of mapping them to public GitHub/GitLab.
3. Fix clean-merge handling before invoking the resolver, or make `allow_no_conflict_state` return success when merge state exists but `unmerged_files()` is empty.
4. Always attach `RemoteReport` for remote runs and print the branch/path summary in human mode.
5. Implement real subprocess timeouts and add the missing acceptance tests named in the spec.

Verification: `cargo fmt --check` passed. `git diff --check main...HEAD` failed on trailing whitespace. I did not run `cargo test`/`clippy` in this read-only review sandbox.
