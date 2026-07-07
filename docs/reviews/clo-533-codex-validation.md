# Pre-PR validation: clo-533

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

**HIGH** - Default local-only remote resolve loses the resolved branch.
AC7 requires default runs to print the scratch repo path and branch name, with no push/comment. The implementation commits into a `TempDir` scratch repo, does not include the scratch path in `RemoteReport`, and the human output only prints branch/base/source/status. When `run_resolve_remote_opt` returns, the `TempDir` is dropped, so a default non-pushed resolution is effectively discarded. See [spec](/Users/mk/Code/gcm--feat-clo-533-remote-mr/docs/specs/2026-07-07-clo-533-remote-mrpr-conflict-orchestration.md:28), [ScratchRepo](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/fetch.rs:23), [remote flow](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/mod.rs:55), and [human output](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/main.rs:166).

**MEDIUM** - Shell-out timeout wrappers can deadlock on verbose commands.
`run_timed` pipes stdout/stderr, waits with `try_wait()`, and only drains the pipes after the child exits. A verbose `git clone`, `gh`, or `glab` command can fill a pipe and block before exit, causing a false timeout. Same pattern exists in publish. See [fetch.rs](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/fetch.rs:257) and [publish.rs](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/publish.rs:109).

**MEDIUM** - Remote clone URL reconstruction drops important URL components.
`RemoteRef.domain` is populated with `Url::host_str()`, then `format_origin_url` rebuilds `https://{domain}/{owner}/{repo}`. That loses ports and other original URL details, so self-hosted URLs like `https://gitlab.example:8443/group/app/-/merge_requests/1` clone the wrong remote. This weakens AC2/self-hosted support and AC5 orchestration. See [host parsing](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/host.rs:181) and [clone URL build](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/fetch.rs:82).

**LOW** - Report host can disagree with the actual parsed host on mismatched flag/URL input.
`extract_remote_arg` returns host from `--pr`/`--mr`, but `resolve_remote_ref` can parse a different host family from the URL; the branch uses `remote_ref.host`, while the report uses the flag-derived `host`. Either reject mismatches or report `remote_ref.host`. See [extract_remote_arg](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/mod.rs:159) and [report assignment](/Users/mk/Code/gcm--feat-clo-533-remote-mr/src/resolve/remote/mod.rs:117).

## Missing Items

- AC7 is not complete: no scratch repo path is printed or serialized, and the default non-pushed result is not durable.
- AC5/AC10 acceptance coverage is incomplete: there is no `merge_produces_conflicts` test, and `partial_escalation_report` only checks dry-run JSON shape, not a real partial escalation.
- AC13 error-path cleanup is not covered by the requested `scratch_cleanup_on_error` test name; only success/user-repo isolation is covered.
- I did not run `cargo test`/`clippy` because this review environment is read-only and Rust builds need to write to `target`.

## Recommendations

- Resolve the AC7/AC13 design conflict explicitly. Either preserve the scratch repo on successful default local-only runs and print its path, or change the product contract so default runs emit a patch/bundle or require `--remote-push` for durable output.
- Replace the polling pipe wrappers with a timeout implementation that drains stdout/stderr concurrently, or avoid piping noisy streams when output is not needed.
- Preserve the original clone URL or store `scheme`, `host`, `port`, and project path separately instead of reconstructing from `host_str()`.
- Add real non-dry-run conflict tests for merge-to-core, partial escalation, checkout failure propagation, and cleanup-on-error.
