# Pre-PR validation: clo-533

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- HIGH: AC13 is not implemented as written. The spec requires scratch cleanup on every exit path, but success calls `TempDir::keep()` and reports the preserved path. That leaves a cloned repo behind by design. See [spec](</Users/mk/Code/gcm--feat-clo-533-remote-mr/docs/specs/2026-07-07-clo-533-remote-mrpr-conflict-orchestration.md:40>), [fetch.rs](</Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/fetch.rs:34>), and [mod.rs](</Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/mod.rs:125>). The spec conflicts with AC7/EC6 here, but the branch currently fails AC13.

- HIGH: The timeout wrappers can deadlock healthy commands. `git`, `gh`, and `glab` are spawned with piped stdout/stderr, but output is only drained after `try_wait()` reports process exit. A verbose `git clone`, `fetch`, `push`, or host CLI command can fill the pipe and block until the wrapper times out. See [fetch.rs](</Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/fetch.rs:277>) and [publish.rs](</Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/publish.rs:108>).

- MEDIUM: The selected `--pr`/`--mr` host is ignored during host detection. `resolve_remote_ref` receives `preferred_host`, but `detect_host` discards it, so `gcm resolve --pr <gitlab-url>` can silently use GitLab/glab instead of enforcing GitHub PR semantics and missing-`gh` behavior. See [host.rs](</Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/host.rs:62>) and [host.rs](</Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/host.rs:238>).

- MEDIUM: The conflict-producing remote path is not covered end to end. The tests named for push/comment are dry-run checks, `partial_escalation_report` only verifies dry-run JSON shape, and the non-dry-run tests use clean merges. That leaves AC5 and AC10 largely unproven. See [tests/resolve_remote.rs](</Users/mk/Code/gcm--feat-clo-533-remote-mr/tests/resolve_remote.rs:760>) and [tests/resolve_remote.rs](</Users/mk/Code/gcm--feat-clo-533-remote-mr/tests/resolve_remote.rs:888>).

- LOW: `git diff --check main...HEAD` fails due trailing whitespace in new docs/review files, including the spec. `cargo fmt --check` passed.

## Missing Items

- AC5: no `merge_produces_conflicts` test exists.
- AC10: no real partial escalation test verifies unresolved files and retained conflict markers.
- AC13: no `scratch_cleanup_on_error` test exists, and success cleanup is contradicted by implementation.
- ST5 mentions `remote_report_json_shape`, but I did not find that test.
- I did not run `cargo test` or `cargo clippy` because this sandbox is read-only and those need build artifacts.

## Recommendations

- Resolve the AC7 vs AC13 contract first. Either preserve a user-visible worktree intentionally and remove AC13, or clean temp dirs and provide another way to access/push the resolution branch.
- Replace the manual `try_wait()` loops with a helper that drains stdout/stderr concurrently while enforcing timeout.
- Enforce `--pr => GitHub/gh` and `--mr => GitLab/glab`, or document and test cross-host URL auto-detection explicitly.
- Add a fake provider or deterministic resolver test path so remote conflicted merges, partial escalation, marker retention, push, and comment behavior are exercised without network calls.
- Clean trailing whitespace or stop using Markdown hard-break spaces in tracked docs if `git diff --check` is expected to stay clean.
