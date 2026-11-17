# Spec Review: clo-940

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-11-17
**Pipeline**: lok spec-review
**Note**: Both external reviewers failed; this is the fallback review

---

I checked the spec against the code. The core diagnosis is correct, but one gap needs a decision before implementation starts.

# Spec review: CLO-940 `--plan-only --all` preview

## 1. Problem Statement

The diagnosis is correct. The early return at `src/main.rs:917-927` exists, and its comment says "no provider call needed". The `--dry-run --all` branch at `:977` previews and returns before `cache::clear`. `Envelope` (`src/output.rs:29-52`) has no field for a message that has not been committed. The `--help` text at `src/cli.rs:69-72` matches the quote. The owner's two decisions are stated clearly.

The statement leaves out who else calls `single_commit_path`. The `--all` flag is not the only caller (`src/main.rs:471`):

- **Merge in progress:** a clean merge in progress (`repo.is_merging()`) also goes there, without `--all`.
- **Grouping fallback:** `run_fallback` (`src/main.rs:1127`) calls it when grouping fails or a cached plan is invalid. It returns any envelope that is not `committed` unchanged.

So the early return also covers `--plan-only` without `--all` on those two paths. Section 4 depends on this point.

## 2. Acceptance Criteria

AC-1, AC-2, AC-3 and AC-5 can be tested, and the mock server supports them. The normal route returns `chore(test): mock commit message` (`scripts/acceptance.sh:123`), and `/fail500/` exists (`:81`). An error envelope gives exit 1.

- **AC-1:** "One provider call" is in the criterion, but Evaluation row 4 does not check it. Count the request bodies in the mock's `$CAPTURE` file.
- **AC-4:** The list leaves out `fallback` and `committed_group`. Both use `Envelope::base`, so they are safe, but the list must name them.
- **AC-4 wording is unclear:** "Grouped `--dry-run` previews" can mean the envelope mode or how the user ran gcm. A grouped `--dry-run` that falls back already returns `status: plan, mode: single` from the preview branch. With the new builder, that envelope will carry `message`. The spec must say whether this is intended.
- **AC-6:** This criterion only repeats the tests from Evaluation. That is acceptable.

## 3. Constraints and Assumptions

- **Must-not conflicts with Prefer.** The Must-not says not to change "`--plan-only` ... without `--all`". The preferred change (remove the early return and use `dry_run || plan_only`) changes `--plan-only` without `--all` on the merge and fallback paths. Today those paths return an empty single plan and make no message call. After the change they make one more provider call and return a `message`. The Escalate rules do not catch this, because no existing test checks the call count there.
  - AC-493-1 and AC-493-2 run grouped `--plan-only` with a valid `PLAN_FILE`, so they will not see the change.
- **The second Escalate rule does not apply.** No code outside `src/output.rs` builds `Envelope` with a struct literal, and `tests/library_*` do not use it. Adding the field only changes `Envelope::base`.
- **One comment goes stale.** `src/main.rs:1245` says "the message is not part of the stable envelope". The comment is not wrong yet, but it becomes stale. The spec does not list it.
- **The `noop_mode` edge case is correct.** On a clean tree, `--plan-only --all` gives `mode: "plan_only"`, not `single`. The spec does not claim a mode there.

## 4. Decomposition / Phases

The three sub-tasks and their order are correct. The new builder keeps `plan: {groups: []}`, `changed_files` and `cached: false`, so the current AC-493-10 assertions still pass.

- **Missing test:** If the owner accepts the change to the fallback and merge paths, sub-task 3 needs one case for it, for example `--plan-only --json` with an empty `PLAN_FILE`.
- **File scope:** The "3 files" count is correct if the stale comment is fixed in `main.rs` during sub-task 2.

## 5. Risks and Open Questions

1. **Open question: should `--plan-only` on the merge and fallback paths also preview the message?**
   - **Accept** means one shared preview branch, and the preview shows what the real run would commit. The cost is a second provider call on a failed grouping, the same as `--dry-run` today.
   - **Gate** means keeping the empty-plan return when `!args.all`. That adds a condition, and the preview then differs from the real run.
   - My recommendation is to accept it. Then edit the Must-not, add an AC and add a test. The Linear item puts "`--plan-only` without `--all`" out of scope, so the owner must decide.
2. **Risk:** The edge case "existing plan cache stays byte-identical" has no row in Evaluation. Each test can set its own `GCM_CACHE_DIR`, as in `scripts/acceptance.sh:682`, so a check is cheap.
3. **Risk (low):** Consumers that reject unknown keys will break on `message`. The owner already accepted this, and `v` stays 1.

## Verdict

**NEEDS_REVISION.** The revision is small: one decision and three edits. The diagnosis and the design are sound.

## Priority Actions

1. Decide Q1. Then change the Must-not to say exactly which `--plan-only` paths change. If the answer is accept, add an AC and an acceptance case for the fallback path (empty `PLAN_FILE`, `--plan-only --json`).
2. Make AC-4 clear: say whether a grouped invocation that falls back to `mode: single` carries `message`. Add `fallback` and `committed_group` to the envelopes that must not carry it.
3. In Evaluation row 4, assert exactly one provider request from `$CAPTURE`.
4. Add an Evaluation row for the byte-identical plan cache, using a per-test `GCM_CACHE_DIR`.
5. Update the comment at `src/main.rs:1245` in sub-task 2.
