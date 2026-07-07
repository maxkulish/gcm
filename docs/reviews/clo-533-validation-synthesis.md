# Pre-PR validation: clo-533

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

I now have everything needed to arbitrate. The evidence is conclusive.

**Verified facts:**
- All 16 integration tests in `tests/resolve_remote.rs` run under `--dry-run` (except the two missing-CLI tests, which fail before doing any work). `remote_push_invoked` asserts `"pushed":false`; `remote_comment_invoked` asserts `"commented":false` and that no comment sentinel exists — both assert the *opposite* of their names. `clean_merge_no_conflicts` and `partial_escalation_report` only check dry-run JSON shape.
- `fetch.rs`, `publish.rs`, and `remote/mod.rs` have **no** `#[cfg(test)]` modules. `resolve/mod.rs` unit tests cover glob/hunk/reconstruction only — not the remote path. So the `clean_merge_no_conflicts` comment ("tested at the unit level") is false.
- Two spec-referenced tests don't exist: `merge_produces_conflicts` (AC5) and `scratch_cleanup_on_error` (AC13).
- `remote/mod.rs:63` discards the `checkout -B <resolution_branch>` result with `let _ =`; after `prepare_scratch_repo` the current branch is `base_branch`, so a silent failure lands the merge commit on the base branch while `RemoteReport` still claims the resolution branch (violates AC6).
- `host.rs` `parse_url` sets `owner=segments[0]`, `repo=segments[1]` unconditionally → GitLab subgroup URLs clone the wrong repo.
- `RemoteReport` has no scratch-path field; `main.rs` prints branch/base/source/pushed/commented but no scratch path (AC7 "prints the scratch repo path" unmet). The scratch `TempDir` is dropped on return, so default mode's branch is deleted — consistent with AC13's mandatory cleanup.

Gemini's "all 14 ACs fully covered and verified" is a hallucinated PASS; it did not read the test bodies. Codex's core findings are grounded and correct.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | success=true; findings verified against source. HIGH#2 (dry-run-only tests) and MEDIUM checkout/URL findings confirmed. HIGH#1 partially confirmed but severity overstated (matches AC13 by design). |
| Gemini | OK (unreliable) | success=true but its central claim "All 14 AC fully covered and verified via unit/integration test suites" is factually false — verified against `tests/resolve_remote.rs` and module test coverage. Its LOW security/isolation observations are accurate. |
| Claude fallback | SKIPPED | At least one external reviewer succeeded. |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- **Test coverage is dry-run placeholders — the entire non-dry-run feature is unproven (Codex HIGH#2, CONFIRMED).** Every integration test uses `--dry-run`; `remote_push_invoked`/`remote_comment_invoked` assert the *opposite* of their names; no unit tests exist for `prepare_scratch_repo`/`commit_resolution`/`push_resolution_branch`/`post_comment`. AC5, AC6, AC9, AC10, AC13, AC14 are effectively unverified, and the spec-named `merge_produces_conflicts` and `scratch_cleanup_on_error` tests are absent. Add real fake-CLI integration tests that drive clone→checkout→merge→commit and assert: resolution branch exists with merged tree, `git push` invoked / `pushed:true`, comment sentinel written / `commented:true`, scratch dir removed after run, and the branch/base guarantees of AC6. The existing `build_fake_remote_clean` fixture needs no LLM provider and can immediately cover AC9/AC13/AC14/AC5/AC6; conflict/escalation paths (AC10) need a fake provider following the Phase-1 mock pattern.
- **Ignored resolution-branch checkout error (Codex MEDIUM#4, CONFIRMED).** `remote/mod.rs:63` — change `let _ = ...checkout -B...` to `?`. A silent failure currently commits the merge onto `base_branch` while the report claims the resolution branch, violating AC6. Add a regression test. One-line fix.
- **GitLab subgroup URL parsing clones the wrong repo (Codex MEDIUM#3, CONFIRMED, bounded).** `parse_url` hardcodes `owner=segments[0]`/`repo=segments[1]`. For GitLab, derive the project path as everything before `/-/`; for GitHub, everything before `/pull/`. Add subgroup and malformed-path test cases. (Malformed-URL *rejection* like `/acme/pull/42` is desirable but secondary — see Deferred.)

## Out of Scope / Deferred
- **AC7 "prints the scratch repo path" is unmet, and default mode's branch is ephemeral.** The `TempDir` is dropped (deleted) on return, so the default-mode resolution branch is not durable and no scratch path is printed. This is *consistent with* AC13 (mandatory cleanup on every exit) — the code follows the spec, but the spec itself has an internal AC7-vs-AC13 tension. Adding a `scratch_path` field to `RemoteReport` + printing it is a trivial bounded add if you want to honor AC7 literally; whether default mode should leave a durable artifact is a **design decision for the user** (see Recommendation), not a code defect blocking this PR.
- **Dead code cleanup (Gemini rec #1).** `publish::publish()` and `run_resolve_remote()` are `#[allow(dead_code)]`. Delete or wire them; non-blocking.
- **Malformed-URL rejection** (e.g. `/acme/pull/42` parsing `repo="pull"`) — robustness hardening, not in the AC surface.

## False Positives / Tooling Artifacts
- **Codex HIGH#1 severity ("loses the only local result").** The data-loss framing overstates it: the ephemeral-branch behavior is the direct consequence of AC13's mandatory scratch cleanup, which the code implements correctly. Reclassified as an AC7 output gap + spec design note, not a HIGH defect.
- **Codex LOW#5 (uncommitted `docs/status/clo-533-workflow.yaml` + trailing whitespace).** This is the orchestrator's own workflow-tracking file, not feature code; it is expected to be uncommitted/managed by the workflow. Trailing-whitespace `git diff --check` is on doc content only. Housekeeping, not a blocker — discard or exclude before the PR is cut.
- **Gemini's PASS verdict and "all 14 AC verified" claim.** Contradicted by direct inspection of the test file; disregarded in this synthesis.
- **Gemini rec #2 (comment error isolation).** Already correctly implemented (`remote/mod.rs:105-114`, EC7); no action.

## Recommendation
PROCEED_WITH_FIXES. The feature *code* is architecturally sound and faithful to the design (clean Phase-1 reuse, isolated scratch clone, credential helper, timeouts, stderr/stdout separation, RAII cleanup), so this is not a pivot. The bounded fix iteration is: (1) `let _ =` → `?` on the resolution-branch checkout + regression test; (2) fix GitLab subgroup path parsing + tests; (3) replace the dry-run placeholder tests with real fake-CLI integration tests that actually exercise merge/commit/push/comment/cleanup — starting with the no-provider `build_fake_remote_clean` fixture for AC9/AC13/AC14/AC5/AC6, then a fake provider for AC10. Be aware the test rewrite is where residual bugs in the never-executed non-dry-run path will surface; budget for small follow-on fixes within the same iteration. One design question is worth a quick check with the user before finalizing: **should default (no `--remote-push`) mode leave a durable artifact, or is ephemeral-preview-then-cleanup the intended behavior?** The current AC7/AC13 wording conflicts, and the answer determines whether you add a persisted-branch/scratch-path path or just document default mode as a preview.
