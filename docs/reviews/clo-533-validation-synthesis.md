# Pre-PR validation: clo-533

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

I've verified all four Codex findings and the coverage dispute against the actual code. I'm ready to synthesize.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Produced a complete review. Could not run `cargo test`/`clippy` (read-only env) - build health unverified by this reviewer. |
| Gemini | OK | Produced a complete review, but its "all 14 ACs verified" claim is overstated (see below). |
| Claude fallback | SKIPPED | Both external reviewers succeeded. |

## Verdict
FAIL

The two reviewers split PASS vs FAIL. I read the code to adjudicate, and the split is genuine: Gemini reviewed against the *dominant* design intent (isolated scratch, never leak temp dirs) and passed; Codex caught that this intent **directly contradicts AC7** and failed. The contradiction is real and requires a product decision, which makes this a Pivot.

## Must Fix Before PR

**[PIVOT - needs user decision] AC7 vs AC13 contradiction: the default local-only run silently discards all resolution work.** (Codex HIGH, CONFIRMED)
- AC7 requires: default run "prints the scratch repo path and the resolution branch name and exits."
- AC13 requires: scratch dir removed on *every* exit path, no residual temp dirs.
- Actual behavior (`mod.rs:55-133`, `fetch.rs:23-29`): the default path clones, checks out, merges, runs the LLM engine, and commits to `gcm-resolve-<host>-<n>` **inside a `TempDir`**. On return, `ScratchRepo` drops → `TempDir` deleted → the resolution branch is destroyed. The scratch path is never added to `RemoteReport` (`report.rs:21-30`) and never printed by the human output (`main.rs:166-185`). So a user running the default sees `status: resolved -> branch gcm-resolve-github-42`, but that branch exists nowhere afterward, and AC7's "print the scratch repo path" is literally unimplemented.
- This can't be fixed mechanically because AC7 and AC13 cannot both hold. The product owner must choose: (a) preserve the scratch on successful no-push runs and print its path (relaxing AC13 to error/abort-only cleanup), (b) emit a durable `git bundle`/patch to a stable location, or (c) redefine the default as preview-only and require `--remote-push` for any durable artifact (rewording AC7). Until that's decided, the core deliverable's default mode is undefined.

The following are real, bounded correctness fixes that should ride along in the same rework once the pivot is decided:

- **Report host can disagree with the branch/CLI actually used** (Codex LOW, CONFIRMED). `RemoteReport.host = host` uses the flag-derived host (`mod.rs:117`, always `GitHub` for `--pr`), while branch naming and CLI selection use the URL-parsed `remote_ref.host`. `detect_host` ignores `preferred_host` entirely (`host.rs:238`). On a mismatched `--pr <gitlab-url>`, JSON reports `host: github` while `glab` ran and the branch is `gcm-resolve-gitlab-N`. One-line fix: report `remote_ref.host`.
- **Clone URL reconstruction drops port and scheme** (Codex MEDIUM, CONFIRMED). `format_origin_url` (`fetch.rs:82`) rebuilds `https://{host_str}/...` from `remote_ref.domain = url.host_str()` (`host.rs:183`), discarding any non-standard port and forcing `https`. A self-hosted `https://gitlab.example:8443/...` clones the wrong endpoint. This weakens the AC2 self-hosted goal. Fix: preserve the URL authority (host+port) or reuse the actual origin URL for the bare-id case.

## Out of Scope / Deferred

- **Timeout wrappers can deadlock on high-volume output** (Codex MEDIUM, CONFIRMED as a latent pattern). `run_timed` (`fetch.rs:252`) only drains the stdout/stderr pipes *after* the child exits, so a child that fills the ~64KB pipe buffer blocks forever → false timeout. Practical risk is low for the specific commands used: `git clone`/`git push` suppress transfer progress on a non-tty pipe without `--progress`, and `gh/glab --json` outputs are small. Real hardening bug, but not a happy-path blocker; fix by draining concurrently (threads or `wait_with_output`).
- **Missing AC-named verification tests** (Codex Missing Items, CONFIRMED). `merge_produces_conflicts` (AC5) and `scratch_cleanup_on_error` (AC13) do not exist by name, so their spec-mandated `cargo test ... --exact` commands fail. `default_no_push` (AC7), `partial_escalation_report` (AC10), `clean_merge_no_conflicts` (AC14), and `remote_push/comment_invoked` (AC9) are **dry-run JSON-shape checks**, not real behavioral tests - they assert `pushed:false`/`status:noop`/field presence, per the tests' own comments. The `real_*` integration tests (`real_clean_merge_resolves_and_commits`, `real_push_invoked`, `real_comment_invoked`, `real_scratch_cleanup_on_success`, `real_resolution_branch_created`) do give genuine end-to-end coverage, so Gemini's "22 robust tests" is directionally right - but its "all 14 ACs verified" is false. Error-path cleanup and real partial-escalation are genuinely untested. Tighten in the same iteration if the branch is reworked anyway.
- **AC12 build health unverified.** Neither external reviewer ran `cargo fmt --check`/`clippy -D warnings`/`test` (read-only env). Confirm green before PR.
- Gemini housekeeping notes (remove `#[allow(dead_code)]` `publish` helper; strip `docs/reviews/*` and `docs/status/clo-533-workflow.yaml` tracking artifacts before merge) - valid, non-blocking.

## False Positives / Tooling Artifacts

- None. Every Codex finding reproduced in the code. Gemini's positive observations (subgroup parsing at `host.rs:224-227`, credential-helper inheritance at `fetch.rs:95-105`, `?`-propagated checkout errors, RAII cleanup on error/abort) are all accurate - RAII drop does guarantee scratch removal on error paths even though no test named `scratch_cleanup_on_error` asserts it. Gemini's only real error is the completeness overclaim on test coverage, not a fabricated finding.

## Recommendation

**STOP_FOR_USER.** The blocker is not a coding defect but an unresolvable contradiction between AC7 ("print the scratch repo path and exit") and AC13 ("remove the scratch dir on every exit path"). The implementer silently chose AC13, which makes the default local-only mode do a full clone-merge-resolve-commit and then throw the resulting branch away with no durable artifact - the opposite of what a user reading AC7 expects. Decide the default's contract: (a) keep the scratch and print its path on successful no-push runs, (b) emit a durable bundle/patch, or (c) make default a preview and require `--remote-push` for durable output. Once that one decision is made, the fix plus the two bounded correctness items (report host, clone URL authority) and the AC5/AC10/AC13 test gaps fit in a single rework iteration. Do not transition to PR until the AC7/AC13 contract is settled.
