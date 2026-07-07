# Pre-PR validation: clo-533

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

I now have full grounding: I read the spec, all four implementation files, the test file, and independently ran the build. Here is the decisive signal neither reviewer had:

- `cargo fmt --check`: clean
- `cargo clippy --all-targets -- -D warnings`: clean (0 warnings)
- `cargo test --test resolve_remote`: 22/22 pass (including the non-dry-run `real_*` tests)
- `cargo test --bins`: 345/345 pass (AC11 Phase-1 core intact)
- `git diff --check`: confirms trailing whitespace in docs only

This lets me adjudicate the Codex/Gemini split from evidence rather than either reviewer's assertion.

---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Full report produced (success=true); could not run `cargo test`/`clippy` (read-only sandbox) - I ran them instead |
| Gemini | OK | Full report produced (success=true); PASS verdict |
| Claude fallback | SKIPPED | At least one external reviewer succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR

1. **Pipe-drain deadlock in the timeout wrappers** - `run_timed` (`src/resolve/remote/fetch.rs:277`) and `run_cmd_timed` (`src/resolve/remote/publish.rs:108`) spawn `git`/`gh`/`glab` with piped stdout+stderr but only read the pipes *after* `try_wait()` reports exit. A child that writes past the OS pipe buffer (~16-64 KB) on a non-tty blocks on write while the parent blocks in the poll loop, so a healthy `git clone`/`fetch`/`push` or a chatty server-hook response deadlocks until the wrapper kills it and reports a false "timed out". Bounded fix: drain stdout/stderr concurrently (reader threads, or `wait_with_output` on a timeout thread). Confirmed by reading; Codex is correct here, Gemini missed it. This is the single strongest finding.

2. **Conflict-resolution path (AC5/AC10) is unproven end-to-end** - every non-dry-run test uses the clean-merge fixture (`build_fake_remote_clean`). The conflicting fixture (`build_fake_remote`) is only ever driven with `--dry-run`, which short-circuits before cloning. So the wrapper's conflict branch (`src/resolve/remote/mod.rs:80-89`: merge `Err` -> `unmerged_files()` -> re-invoke core) has zero coverage, and `partial_escalation_report` (`tests/resolve_remote.rs:887`) only asserts JSON shape. This path is deterministically testable with no provider (no provider -> core escalates all conflicts). Bounded fix: one non-dry-run conflicting-merge test asserting escalated files are listed and conflict markers remain on `gcm-resolve-*`.

Both fixes are independent, small, and land in one iteration.

## Out of Scope / Deferred

- **AC13 vs AC7/EC6 spec contradiction (Codex HIGH #1).** Real, but it is a spec-internal inconsistency, not a code defect. AC7 requires printing the scratch path and EC6 requires the resolution branch to remain "in the scratch repo so the user can push manually" - both mandate preservation on success. AC13's prose ("removed on every exit path... success") contradicts them. The implementation resolved this correctly: `keep()` on success (`fetch.rs:37`), `TempDir` drop on error/abort. The AC13 verification test is `scratch_cleanup_on_error`, which covers only the error path and passes. Action: amend AC13 wording to carve out the success path; no code change. The temp-dir retention on success is intentional, not a leak.
- **`preferred_host` ignored in `detect_host` (`host.rs:238`, Codex MEDIUM).** The URL domain is authoritative, so a mistyped `--pr`/`--mr` auto-corrects to the URL's actual host and neutral-domain self-hosted instances cannot be forced. Low impact off the happy path. Cheap to fold into the fix iteration (enforce `--pr`⇒GitHub / `--mr`⇒GitLab, or use as a fallback) but not a blocker.
- **Trailing whitespace in tracked docs (Codex LOW #5).** Confirmed by `git diff --check` - markdown hard-break spaces in the spec and review files. Cosmetic.
- **Dead-code helpers behind `#[allow(dead_code)]`** (`run_resolve_remote` `mod.rs:35`, `publish()` `publish.rs:32`, `path()` `fetch.rs:43`; Gemini LOW). Minor API cleanup.

## False Positives / Tooling Artifacts

- **Codex: "AC9 push/comment largely unproven, dry-run only."** Overlooked `real_push_invoked` (`tests/resolve_remote.rs:1047`) and `real_comment_invoked` (`:1085`), which are non-dry-run, push to the fake remote and assert the branch appears, and assert the `gh pr comment` sentinel fires. Push and comment *are* proven end-to-end on the clean-merge path. Only the *conflict* path is genuinely uncovered (captured as Must Fix #2).
- **Codex: "`remote_report_json_shape` (ST5) missing."** The literally-named test is absent, but the JSON `remote` block shape is asserted by `partial_escalation_report` and `real_clean_merge_resolves_and_commits`. Naming gap, not a coverage gap.
- **Codex: "did not run `cargo test`/`clippy`."** I ran them: fmt clean, clippy `-D warnings` clean, 22/22 integration + 345/345 unit tests pass. AC11 and AC12 are satisfied - which invalidates the basis for a hard FAIL.
- **Codex overall FAIL** rested substantially on AC13 counted as a code failure; it is a spec contradiction the code resolved sensibly. **Gemini overall PASS** was too lenient - it missed the deadlock and the untested conflict branch.

## Recommendation

PROCEED_WITH_FIXES. The change is faithful to the design, builds clean, and passes fmt/clippy/all 367 tests - there is no pivot and no material divergence, so FAIL is not warranted. Address two bounded fixes in one iteration before opening the PR: (1) drain stdout/stderr concurrently in `run_timed`/`run_cmd_timed` to remove the pipe-fill deadlock, and (2) add a non-dry-run conflicting-merge test that exercises `mod.rs:80-89` and asserts AC5/AC10 (escalated files listed, markers retained). While in there, cheaply fold in the whitespace cleanup, the AC13 spec-wording correction, and optionally the `preferred_host` enforcement. None of these require a user decision.

---

## Re-validation

Applied the single synthesis-approved fix iteration before PR:

1. **Pipe-drain deadlock fixed.** `run_timed` in `src/resolve/remote/fetch.rs` and `run_cmd_timed` in `src/resolve/remote/publish.rs` now use `wait_with_output()` on a worker thread and `recv_timeout()` for bounded waits, so stdout/stderr are drained while the child runs instead of after `try_wait()` reports exit.
2. **Conflict path covered end-to-end.** Added `resolve_remote::real_merge_produces_conflicts`, a non-dry-run conflicting PR test with a fake Ollama endpoint. It asserts a `partial` report, `f.txt` escalated with one escalated hunk, the preserved `gcm-resolve-github-42` scratch branch, and retained conflict markers.
3. **Related cleanup folded in.** AC13 wording now carves out the intentional successful local-only scratch preservation; trailing markdown whitespace was removed from tracked spec/review docs.

Re-run gate after fixes:

- `cargo fmt --check`: PASS
- `cargo clippy -- -D warnings`: PASS
- `cargo test`: PASS (407 total tests; `resolve_remote` 24/24)

The `PASS_WITH_NOTES` verdict remains valid with the required fix iteration applied.
