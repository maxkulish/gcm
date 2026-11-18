# Spec: `--plan-only --all` previews the single-commit message

**Created**: 2026-11-17
**Linear**: [CLO-940](https://linear.app/cloud-ai/issue/CLO-940)
**Estimated scope**: S (3 files, 3 sub-tasks)

## 1. Problem Statement

`gcm --help` documents `--plan-only` as "Generate the plan (or single-commit preview with --all) and exit without staging, committing, or touching the cache" (`src/cli.rs:69-72`). In v0.8.5, `--plan-only --all` makes no provider call:

- Human mode prints nothing and exits 0.
- JSON mode returns `{"v":1,"status":"plan","mode":"single","plan":{"groups":[]},"changed_files":[...],"cached":false}` with no message.

The cause is an early return in the single-commit path of `src/main.rs`. At `src/main.rs:917-927`, `if args.plan_only` returns `output::plan(..., MODE_SINGLE, Plan { groups: vec![] }, ...)` before the diff is gathered and before `provider.generate_message` runs (`src/main.rs:968`). The comment there says "no provider call needed", which contradicts `--help`.

`--dry-run --all` works in human mode: it calls the provider, then `src/main.rs:977-988` prints the message with `ui_preview` (`src/main.rs:1180`) and returns before `cache::clear` (`src/main.rs:992`) and before the index snapshot. In JSON mode, `--dry-run --all` has the same gap: the envelope carries no message, because `Envelope` (`src/output.rs:29-52`) has no field for an uncommitted message. `CommitSummary.message` describes a real commit only.

Agents and scripts use `--plan-only --json` as the non-destructive preview. For the `--all` case they get no message.

The early return sits in `single_commit_path` (`src/main.rs:904`), which has three callers: `--all` and a merge in progress (`src/main.rs:471`), and the grouping fallback through `run_fallback` (`src/main.rs:1127`). A merge in progress or a grouping fallback reaches the early return without `--all`.

The owner made three decisions on 2026-11-17:

- The JSON envelope gets a new optional top-level `message` field, present only on a single-commit preview. The change is additive, so `v` stays 1.
- Every single-commit preview envelope carries the field. `--plan-only --all` and `--dry-run --all` both carry it, and so do the merge and fallback paths under `--dry-run`, because they already call the provider today.
- `--plan-only` without `--all` keeps its current behavior on the merge and fallback paths: no message-generation call, an empty plan and no `message`. Only `--plan-only --all` changes.

## 2. Acceptance Criteria

- [ ] AC-1: `gcm --plan-only --all` on a dirty tree makes one single-commit message provider call, prints the generated message in the same block as `--dry-run --all`, exits 0, and leaves the index, HEAD and plan cache unchanged.
- [ ] AC-2: `gcm --plan-only --all --json` returns `v: 1`, `status: "plan"`, `mode: "single"`, `plan.groups: []`, `cached: false`, `changed_files` equal to the fixture paths, and a top-level `message` equal to the generated message.
- [ ] AC-3: Every single-commit `--dry-run` preview carries the generated message in JSON mode: `--dry-run --all --json`, `--dry-run --json` on a clean merge in progress, and a grouped `--dry-run --json` whose plan does not parse (fallback). In each case `message` equals the generated message.
- [ ] AC-4: The `message` key appears only on an envelope with `status: "plan"` and `mode: "single"` that holds a generated message. It does not appear on grouped previews (`mode: "plan_only"` / `"dry_run"`), nor on `committed`, `committed_group`, `fallback`, `noop` or `error` envelopes.
- [ ] AC-5: A failure returned by `provider.generate_message` under `--plan-only --all` gives exit 1 (the same as `--dry-run --all`), and in JSON mode an `error` envelope with `mode: "single"` and no `message` key. The index is unchanged. Errors raised before generation, such as provider selection (`src/main.rs:413`, mode from `mode_from_args`), keep their current envelope.
- [ ] AC-6: `--plan-only` without `--all` is unchanged on the merge and fallback paths. On a clean merge in progress, `--plan-only --json` makes zero provider requests. On a grouped run whose plan does not parse, `--plan-only --json` makes exactly one request (the grouping request). Both return `status: "plan"`, `mode: "single"`, an empty plan and no `message`.
- [ ] AC-7: Tests cover the human and JSON outputs of `--plan-only --all`, the three `--dry-run` previews of AC-3, the provider error case, and the unchanged merge and fallback cases, with request counts.

**Verification method**: `cargo test` for unit tests in `src/output.rs`; `scripts/acceptance.sh` for the end-to-end cases against its mock server (normal route for success, `/fail500/` route for the provider error); `cargo fmt --check` and `cargo clippy -- -D warnings`. Run `cargo build --release` before `scripts/acceptance.sh`, because the script reuses an existing binary (`scripts/acceptance.sh:25`).

## 3. Constraints

**Must**:
- Reuse the existing single-commit flow: gather diff, privacy, budget check, `generate_message`, then the preview branch. `--plan-only --all` and `--dry-run --all` take the same branch.
- Return from the preview branch before `cache::clear(repo)` and before `repo.snapshot_index()`.
- Add `message: Option<String>` to `Envelope` with `#[serde(skip_serializing_if = "Option::is_none")]`, initialized to `None` in `Envelope::base`.
- Set `message` only through one builder for the single-commit preview (for example `output::single_preview(provider, model, message, changed_files)`), so no other envelope can carry it.
- Keep `mode: "single"`, `plan: {"groups": []}`, `changed_files` and `cached: false` in the single preview envelope, so existing consumers (acceptance AC-493-10) keep working.
- Keep `v` at 1.

**Must-not**:
- Change `--plan-only` without `--all`, on the grouped path, the merge path or the fallback path.
- Change the provider calls or human output of `--dry-run` on any path. Its JSON envelope changes only on single-commit previews, where it gains `message`.
- Rename or remove any existing envelope field, or change `CommitSummary`.
- Touch the grouping one-file EOF behavior (CLO-941).
- Change error envelopes raised before message generation (such as provider selection), or the `--reset` cache clearing (acceptance AC-493-13).
- Change the human preview text of `ui_preview` or `ui::preview_plan`.

**Prefer**:
- Narrow the early return at `src/main.rs:917` to `if args.plan_only && !args.all`, and change the existing `if args.dry_run` at `:977` to `if args.dry_run || args.plan_only`. Update the "no provider call needed" comment so it names the merge and fallback paths.
- Extend the existing acceptance case AC-493-10 instead of adding a parallel case.

**Escalate when**:
- An existing test or acceptance case depends on `--plan-only --all` making no provider call.

## 4. Decomposition

1. **Envelope field and builder**: add `message: Option<String>` to `Envelope`, set `None` in `Envelope::base`, add the single-preview builder, add unit tests: the builder sets `message`, and `plan`, `noop`, `committed`, `error` serialize without a `message` key. Files: `src/output.rs`.
2. **Single-commit preview path**: narrow the early return to `args.plan_only && !args.all`, run the preview branch for `args.dry_run || args.plan_only`, and return the new builder's envelope. Update the comment at the early return and the stale comment at `src/main.rs:1245` ("the message is not part of the stable envelope"). Files: `src/main.rs`.
3. **Acceptance coverage**: extend AC-493-10 to assert `v == 1`, `changed_files` equal to `a.txt` and `b.txt`, `plan.groups == []`, `cached == false` and the exact mock message. Add:
   - a human-mode `--plan-only --all` case: message printed, one request in `$CAPTURE`, index and HEAD unchanged;
   - a cache case: in one repo and one `GCM_CACHE_DIR`, warm a plan with grouped `--dry-run`, copy the `plan-*.json` file, run `--plan-only --all`, then `cmp` the file;
   - a `--dry-run --all --json` case with the exact message;
   - a clean-merge pair on the AC-G12c fixture (`scripts/acceptance.sh:751`): `--plan-only --json` with zero requests and no `message`, then `--dry-run --json` with one request and the exact message;
   - an empty-`PLAN_FILE` fallback pair: grouped `--plan-only --json` with one request and no `message`, then grouped `--dry-run --json` with two requests and the exact message;
   - a grouped `--plan-only --json` "no message key" assertion with a valid plan;
   - a `/fail500/` `--plan-only --all --json` error case.

   The preview cases do not commit, so they need the `SIGNING_OK` gate only if the fixture setup needs it. Files: `scripts/acceptance.sh`.

**Dependency order**: 1 before 2; 3 after 2.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Single-preview builder sets `message` | Serialized JSON has `"message":"<text>"`, `status` plan, `mode` single | `cargo test output::` |
| 2 | Other builders omit `message` | `plan`, `noop`, `committed`, `committed_group`, `fallback`, `error` JSON has no `message` key | `cargo test output::` |
| 3 | `--all --plan-only --json` (AC-493-10 extended) | exit 0, `v == 1`, status plan, mode single, `changed_files == ["a.txt","b.txt"]`, `plan.groups == []`, `cached == false`, `message == "chore(test): mock commit message"` | `scripts/acceptance.sh` |
| 4 | `--all --plan-only` human | stdout holds the mock message; exactly one line in `$CAPTURE`; `write-tree` and HEAD unchanged | `scripts/acceptance.sh` |
| 5 | Plan cache untouched | in one repo and one per-test `GCM_CACHE_DIR`, a grouped `--dry-run` writes `plan-*.json`; after `--all --plan-only` that file is byte-identical (`cmp`) and no new cache file appears | `scripts/acceptance.sh` |
| 6 | `--all --dry-run --json` | exit 0, `message == "chore(test): mock commit message"` | `scripts/acceptance.sh` |
| 7 | Clean merge, `--plan-only --json` | exit 0, status plan, mode single, no `message`, zero lines in `$CAPTURE` | `scripts/acceptance.sh` |
| 8 | Clean merge, `--dry-run --json` | exit 0, `message == "chore(test): mock commit message"`, one line in `$CAPTURE`, MERGE_HEAD still set | `scripts/acceptance.sh` |
| 9 | Grouped `--plan-only --json` with a valid plan | no `message` key | `scripts/acceptance.sh` |
| 10 | Grouped `--plan-only --json` with an empty `PLAN_FILE` (fallback) | exit 0, status plan, mode single, no `message`, exactly one line in `$CAPTURE` | `scripts/acceptance.sh` |
| 11 | Grouped `--dry-run --json` with an empty `PLAN_FILE` (fallback) | exit 0, status plan, mode single, `message == "chore(test): mock commit message"`, two lines in `$CAPTURE` | `scripts/acceptance.sh` |
| 12 | `--all --plan-only --json` with `/fail500/` | exit 1, status error, mode single, no `message`; index unchanged | `scripts/acceptance.sh` |
| 13 | `--reset --plan-only --json` (AC-493-13) | unchanged: cache cleared, exit 0 | `scripts/acceptance.sh` |
| 14 | Lint, format, unit tests | clean | `cargo fmt --check && cargo clippy -- -D warnings && cargo test` |

**Edge cases to verify**:
- Clean tree with `--plan-only --all`: still the `noop` envelope from the early check (`src/main.rs:365`/`:406`), no provider call, no `message`.
- Non-TTY without `--yes`: `--plan-only --all` still passes the terminal check (`src/main.rs:371`), because no confirmation prompt runs.
- An existing plan cache file for the same repo stays byte-identical after `--plan-only --all`. A seed file under another name proves nothing, because `cache::clear` removes only the file derived from the repo root (`src/cache.rs:79`).
- An unsupported model under `--plan-only --all` fails at provider selection with the current `mode: "plan_only"` error envelope (out of scope for AC-5).
