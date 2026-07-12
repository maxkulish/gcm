# Spec: Rework `gcm resolve` into an ownership transaction (CLO-555)

**Created**: 2026-07-12
**Estimated scope**: L (~8 source files + 2 test files, 8 sub-tasks)
**Design source of truth**: `docs/hotfix/2026-07-12-resolve-stage-and-finish.md` (v2, accepted; owner decisions 1-4 recorded). This spec operationalizes that document; on any perceived conflict, the design doc wins and the conflict must be escalated, not silently resolved.

## 1. Problem Statement

`gcm resolve` writes accepted conflict resolutions to the working tree and stops. The index keeps its unmerged stages (`git status` shows `UU`), `MERGE_HEAD` survives, and the user must run `git add` + `git merge --continue` by hand while the headline claims "All conflicts resolved." (field repro 2026-07-12, `~/Code/Vault/personal-health`). Four adjacent defects were confirmed during design review, all with file:line evidence:

1. **Unsafe prompt parser.** `src/ui.rs:44-48` treats anything that is not exactly `n`/`N`/`e`/`E` as Accept - including `no`, `No`, typos, empty Enter, and EOF/Ctrl-D (`read_line` returns `Ok(0)`, empty string falls to `_ => Accept`). This contradicts README.md:362 ("EOF never auto-accepts"); the non-TTY guard (`src/resolve/mod.rs:116-118`) only covers run start. The commit flow's `confirm()` (`src/ui.rs:89-100`) has the identical parser and the identical consequence (an unwanted commit).
2. **Pre-confirmation mutation.** `checkout_conflict_zdiff3` (`src/resolve/mod.rs:103-106`) rewrites every conflicted file before any prompt, silently destroying manual partial resolutions; `mergiraf solve` mutates in place (`src/resolve/mergiraf.rs:48-52`); a fully-successful mergiraf resolution returns `Accepted` without ever showing a prompt (`src/resolve/mod.rs:325-340`).
3. **Uncentralized staging targets.** Marker-free unmerged files (`src/resolve/mod.rs:309-319`) and mergiraf-resolved files return `Accepted` without any `write_file` call, so staging attached to the write sites would miss them.
4. **Remote Partial hazard.** Local and remote share `run_resolve_in_repo` (`src/resolve/remote/mod.rs:81,90`). The remote wrapper's `commit_resolution` (`:99`) runs unconditionally - `git add -A` (`:162`) stages raw conflict markers on a Partial report, `--allow-empty` (`:174`) commits them, and `--remote-push` (`:107-110`) pushes without checking status.

The fix is the contract decided in the design doc:

> **Yes to every file = gcm applies everything, stages it, and finishes the Git operation with a signed commit. No to any file = gcm restores the repository byte-for-byte and exits 0.**

Owner decisions (do not relitigate): (1) tool escalation is not rejection - escalated runs stage confirmed work, skip the finish, and report Partial; (2) both prompts share one parser in this change; (3) the remote Partial commit/push fix rides along; (4) the multi-commit rebase loop is out of scope (CLO-554) - this change completes exactly one conflict stop.

## 2. Acceptance Criteria

- [ ] **AC1 (transaction success)**: In a merge with N conflicted files, answering Yes (or running `--yes`) to all N leaves: zero unmerged index entries, `MERGE_HEAD` absent, `HEAD` a 2-parent commit carrying a signature header, and a headline naming the short sha. Zero manual steps remain before `git push`.
- [ ] **AC2 (abort restores)**: Answering No to any file (including file k of N after k-1 Yeses) restores every unmerged file's exact pre-run bytes (byte-compare, including manual partial edits present before the run), leaves the index and operation refs untouched, exits 0, and prints `Aborted - working tree restored, nothing changed.`
- [ ] **AC3 (safe parser, both prompts)**: Case-insensitive `y`/`yes` accepts, `n`/`no` rejects, `e`/`edit` edits; any other input reprompts up to 3 times then rejects; EOF and empty Enter reject immediately. Prompt text renders `[y/N/e(dit)]`. The same behavior holds for the commit flow's message prompt - **breaking change: Enter previously accepted and now aborts (exit 0)**; it must be flagged prominently in the README and the next release notes.
- [ ] **AC4 (no silent acceptance)**: A mergiraf-resolved file is previewed and confirmed like any LLM-resolved file; rejecting it restores its original markers. Marker-free unmerged files (already resolved by the user before the run) are staged in the apply phase without a prompt.
- [ ] **AC5 (escalation semantics)**: When at least one file escalates (binary, sensitive-path, provider failure, validation escalation) and every non-escalated file is confirmed, gcm writes and stages the confirmed files, does NOT finish, reports Partial, and lists the still-unmerged paths plus the exact next command. `--yes` semantics, spelled out: phase B is skipped and every validated, non-escalated proposal is auto-confirmed; escalation of any file (including the first, before any confirmation) follows the same stage-progress-and-stop rule; `--yes --no-finish` applies + stages and skips the finish. `--yes` can never trigger the abort-restore path (rejection is user-only).
- [ ] **AC6 (signed finish, postcondition-classified)**: The finish step creates the merge commit via `git commit -S --no-edit` (consuming `MERGE_MSG`), or continues rebase/cherry-pick via `git -c commit.gpgsign=true <op> --continue` with `GIT_EDITOR=true`; stdin is inherited (pinentry works); hooks run (no `--no-verify` locally). Outcome is classified by postconditions (operation refs, unmerged set, HEAD movement), not stderr text: `Completed{head_sha}` / `StoppedOnNextConflict` / `Failed{detail}`. A rebase stopping on the next conflicted commit reports it and names `gcm resolve` as the next step.
- [ ] **AC7 (finish failure is safe)**: A rejecting hook (e.g. pre-commit exit 1) leaves the staged state intact, streams the hook output, prints the manual continue command, and exits non-zero.
- [ ] **AC8 (remote gating)**: The engine never stages or finishes in remote mode; the remote wrapper commits and pushes only when the report status is Resolved or Noop. A Partial remote run produces zero commits on the resolution branch and never pushes, even with `--remote-push`. A Resolved remote run produces exactly one commit (no empty duplicate).
- [ ] **AC9 (truthful output)**: Every headline matches the `git status` a user sees next (completed / staged-only / aborted / partial variants per design doc). JSON gains only additive fields: `staged: [paths]`, `finish: {result, commit?}`, `restored: bool`, plus new enum values `status: "aborted"` and `action: "rejected"`; `v` stays `1`. Serialization rule: `restored` is emitted only when `true` (`skip_serializing_if` on false), `staged` only when non-empty, `finish` only when present - a run that touches none of them produces byte-identical JSON to today. In interactive `--json` mode the preview prints to stderr; stdout remains a single JSON object.
- [ ] **AC10 (escape hatch + boundaries)**: `--no-finish` (CLI-only, resolve subcommand) applies + stages but skips the finish and prints the continue hint. Local resolve never pushes. `--dry-run` mutates nothing (no snapshot, no write, no stage, no finish).
- [ ] **AC11 (docs match runtime)**: README is updated by section, not line number (lines drift): the resolve pipeline's "Preview" step, the resolve "Safety guarantees" paragraph, the intro's resolve summary, and the commit-flow prompt documentation ("`Y`/Enter commits group 1") are rewritten to the new contract; `--no-finish` documented; the false "never runs `git add` or `--continue`" and "EOF never auto-accepts" claims removed; the Enter-now-aborts breaking change and the SIGINT limitation (below) get explicit callouts.
- [ ] **AC12 (hygiene)**: Dead `if args.dry_run` arm inside the mergiraf block (`src/resolve/mod.rs:326-330`) removed. `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` green.

**Verification method**: `cargo test` (unit + integration; new tests per Section 5), `cargo clippy -- -D warnings`, `cargo fmt --check`, plus the evaluation table commands. Signing-dependent integration tests follow the repo's established probe-gate pattern (`scripts/acceptance.sh` `probe_signing`): probe `git commit -S` in a throwaway repo, skip with a printed note when unavailable (CI has no key; owner machine does).

## 3. Constraints

**Must**:
- Snapshot the raw bytes of every unmerged file before the first mutation (before `checkout_conflict_zdiff3`); abort-restore writes those exact bytes back. Use byte-level IO (new `Repo::read_file_bytes`/`write_file_bytes` or `std::fs` against `repo.root()`), not the lossy `String` path.
- Collect all confirmations before applying anything: no `write_file`, no `git add`, no finish until every file has a decision (three-phase structure: propose, confirm, apply).
- Stage centrally from the final per-file disposition (covers LLM, edited, mergiraf, and marker-free files); stage exact paths with the `GIT_LITERAL_PATHSPECS=1` guard used by `stage_group` (`src/git.rs:418`); never `git add -A` locally.
- Follow `commit_signed`'s subprocess pattern for the finish helper (`src/git.rs:230-241`): stdin inherited, stderr inherited, stdout captured and re-logged to stderr; `run_git` (`.output()`, stdin null) must not be used for the finish.
- Dispatch the finish by operation, checking rebase first, then cherry-pick, then merge (a stopped rebase/cherry-pick can carry auxiliary merge state; `MERGE_HEAD` alone must not misroute it); return a `NothingToFinish` variant when no operation ref exists (e.g. `git checkout -m` conflicts) and fall back to the staged-only message.
- Classify finish outcomes strictly by postconditions re-read after the subprocess exits: operation refs via the existing `is_merging`/`is_rebasing`/`is_cherry_picking`, unmerged set via `unmerged_files()`, `HEAD` via `rev-parse`. A rebase that continues through the remaining commits cleanly is `Completed` even though it passed intermediate steps.
- Thread an explicit execution context (`Local` vs `Remote`) into `run_resolve_in_repo`; `Remote` behaves exactly as today inside the engine (write-only), and the wrapper gates `commit_resolution` + push on `status == Resolved || status == Noop` (keep `--allow-empty` for the clean-merge Noop case; keep `--no-verify` in the scratch repo only).
- Escalated files keep their zdiff3-marker working-tree state (today's escalation artifact); only a user rejection triggers the byte-for-byte restore.
- User rejection exits 0 with `ResolveStatus::Aborted` + `restored: true`; per-file actions record the decisions made up to the abort (the rejecting file gets `rejected`).
- A finish failure surfaces as a new typed error (e.g. `GcmError::FinishFailed { op, detail }`) with a Display message naming the manual continue command; exit non-zero; staged state untouched. `GcmError::leaves_staged()` (`src/error.rs:63-65`) must return `true` for `FinishFailed` so no error path (present or future) treats it as index-restorable. A signing/pinentry failure during the finish is a `FinishFailed` like any other: staged state kept, manual command printed.
- The abort restore carries an external-modification guard (dependency-free): the transaction records the bytes it last wrote per file; if a file's current content differs at restore time (concurrent edit in another terminal), that file is NOT overwritten - a warning names it and restore continues for the rest.
- Provider validation-retry and provider errors live entirely in phase A (proposal building), exactly as today: the bounded retry runs before any proposal is confirmed; a file whose retry fails becomes an escalated proposal. Phase B and C never call the provider.
- New prompt parser is a pure, unit-testable function plus a `BufRead`-parameterized loop (stdin wrapper at the call site); both `confirm_file` and `confirm` route through it; `--yes` semantics unchanged.
- Keep the `--json` stdout stream a single JSON envelope in every path; all new JSON fields use `skip_serializing_if` so existing consumers see unchanged output when the fields are empty/absent.

**Must-not**:
- No new dependencies; no config key for finish behavior (CLI `--no-finish` only); no `--no-verify` on local finishes; no push from local resolve under any flag combination.
- No SIGINT/signal handler (would require a new dependency, violating the line above). **Known limitation, documented in the README safety guarantees**: Ctrl-C during the confirm or apply phase exits without the snapshot restore; the working tree may hold zdiff3/mergiraf-rewritten files and the recovery command (`git checkout --conflict=merge -- <paths>` or re-running `gcm resolve`) must be named in the README. Reviewer disagreement (handler vs documentation) resolved by the no-new-deps constraint.
- No resolve-until-clean rebase loop (CLO-554); a `StoppedOnNextConflict` ends the run with the documented message.
- No observable change to `--dry-run` behavior, the non-TTY guard (`needs_terminal_but_absent`), provider selection, privacy filtering, or the validation gate. (Removing the mergiraf block's inner dry-run arm is not a behavior change: it is unreachable code, since the enclosing condition already requires `!args.dry_run` - see AC12.)
- Do not remove or rename existing JSON fields or enum values; do not bump `v`.
- Do not restore escalated files from snapshot (only rejection restores).

**Prefer**:
- Reuse `commit_signed`'s runner via a shared private helper rather than duplicating the spawn logic.
- Keep `FileResolution`/report bookkeeping shapes; extend rather than rewrite.
- Tests-first per sub-task (repo convention from CLO-545/531).

**Escalate when**:
- The design doc and this spec appear to conflict, or an owner decision (1-4) seems wrong in practice.
- `git commit -S --no-edit` or `-c commit.gpgsign=true <op> --continue` behaves differently across the git versions in CI (ubuntu-latest, macos-latest) in a way that breaks postcondition classification.
- The three-phase restructure of `resolve_file` forces a change to provider batching or validation-retry semantics (it should not).
- Any test requires weakening the snapshot/restore byte-exactness guarantee.

## 4. Decomposition

1. **ST1 - shared prompt parser**: pure `parse_choice(&str) -> Option<PromptChoice>` (`Yes|No|Edit`), `BufRead` loop with 3-attempt reprompt, EOF/empty = No; route `confirm_file` and `confirm` through it; prompts render `[y/N/e(dit)]`. Unit tests for every AC3 case on both prompts. - files: `src/ui.rs`
2. **ST2 - byte snapshot/restore**: `Repo::read_file_bytes`/`write_file_bytes`; snapshot map of all unmerged paths taken in `run_resolve_in_repo` before zdiff3 (non-dry-run only); `restore_snapshot` helper. Unit tests: round-trip byte-exactness incl. CRLF and no-trailing-newline files. - files: `src/git.rs`, `src/resolve/mod.rs`
3. **ST3 - finish helper**: `FinishOutcome` enum + `Repo::finish_conflict_op()` with the dispatch order, signing, env, stdio, and postcondition rules from Section 3; `GcmError::FinishFailed` + Display. Unit tests in `git.rs` temp repos (merge completed / hook-fail / nothing-to-finish; signing cases probe-gated). - files: `src/git.rs`, `src/error.rs`
4. **ST4a - proposal building**: restructure `resolve_file` into a pure proposal builder (no write, no prompt) returning per-file proposals; mergiraf results and marker-free files become proposals with the right dispositions; validation-retry stays inside this stage; dead dry-run arm removed. - files: `src/resolve/mod.rs`
5. **ST4b - confirm + abort/restore**: confirm-all loop in `run_resolve_in_repo` (preview + ST1 prompt; edit via `$EDITOR` + validation); rejection triggers snapshot restore with the external-modification guard, `Aborted` report, exit 0. - files: `src/resolve/mod.rs`
6. **ST4c - apply + central staging**: write confirmed proposals, stage exact paths by final disposition (LLM, edited, mergiraf, marker-free), escalation path stages confirmed work only. - files: `src/resolve/mod.rs`
7. **ST5 - finish integration + `--no-finish` + execution context**: `ResolveMode { Local, Remote }` threaded through `run_resolve_in_repo`; Local + fully-resolved + not `--no-finish` calls `finish_conflict_op`; escalation and `--no-finish` paths stop after staging; `--no-finish` flag added to the Resolve subcommand. - files: `src/resolve/mod.rs`, `src/cli.rs`, `src/main.rs`
8. **ST6 - remote gating**: wrapper passes `ResolveMode::Remote`; `commit_resolution` + `push_resolution_branch` gated on Resolved/Noop; Partial prints a note, keeps the scratch path, never pushes; an Aborted remote run neither commits, pushes, nor preserves the scratch dir (TempDir default cleanup). - files: `src/resolve/remote/mod.rs`
9. **ST7 - output**: new headlines per exit path; `ResolveStatus::Aborted` + `FileAction::Rejected` enum variants; `staged`/`finish`/`restored` report fields with the AC9 serialization rules; interactive `--json` preview moved to stderr. - files: `src/resolve/report.rs`, `src/resolve/mod.rs`, `src/ui.rs`
10. **ST8 - README + integration test matrix**: README rewrite per AC11 (incl. Enter-aborts breaking-change callout and SIGINT limitation); a Rust-side `signing_available()` test helper mirroring `scripts/acceptance.sh`'s `probe_signing`; end-to-end tests in `tests/resolve_integration.rs` + `tests/resolve_remote.rs` per Section 5. - files: `README.md`, `tests/resolve_integration.rs`, `tests/resolve_remote.rs`

**Dependency order**: ST1, ST2, ST3 are independent of each other. ST4a needs ST2; ST4b needs ST1+ST4a; ST4c needs ST4b; ST5 needs ST3+ST4c; ST6 needs ST5's context; ST7 alongside ST4b-ST5; ST8 last. Suggested serial order: ST1 -> ST2 -> ST3 -> ST4a -> ST4b -> ST4c -> ST5 -> ST6 -> ST7 -> ST8.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Parser: `No`, `no`, `NO`, `q`+`q`+`q`, EOF, empty Enter | all reject; `y`/`yes`/`Y` accept; `e`/`edit` edit; unknown input reprompts max 3 | `cargo test -p gcm parse_choice` (unit, `src/ui.rs`) |
| 2 | Commit-flow prompt shares parser | `no` and EOF abort the commit (exit 0, nothing staged) | unit tests on `confirm` + existing acceptance AC-2 path |
| 3 | All-accept merge finishes signed | index clean, `MERGE_HEAD` gone, 2-parent HEAD with `gpgsig`/`sig` header, headline has short sha, JSON `finish.result=completed` | `cargo test --test resolve_integration transaction_` (probe-gated for the signature assert) |
| 4 | Reject file 2 of 2 | file 1 bytes == pre-run bytes (byte-compare incl. a manual partial edit fixture), index untouched, `MERGE_HEAD` intact, exit 0, JSON `status=aborted`, `restored=true` | `cargo test --test resolve_integration abort_restores` |
| 5 | Mergiraf proposal previewed | mergiraf-resolved file prompts; `n` restores original markers | `cargo test --test resolve_integration mergiraf_confirm` (fake mergiraf on PATH) |
| 6 | Marker-free unmerged file | staged in apply phase without prompt; counted in JSON `staged` | `cargo test --test resolve_integration marker_free_staged` |
| 7 | Escalated + accepted mix (interactive and `--yes`) | accepted file staged, no finish, `MERGE_HEAD` intact, status Partial, remaining path + next command printed | `cargo test --test resolve_integration escalation_stages_progress` |
| 8 | `--no-finish` | staged, `MERGE_HEAD` intact, hint printed, JSON `finish.result=skipped` | `cargo test --test resolve_integration no_finish_flag` |
| 9 | Hook rejects finish | pre-commit exit 1: staged intact, non-zero exit, manual command named | `cargo test --test resolve_integration finish_hook_failure` |
| 10 | Cherry-pick completes; 2-conflict rebase stops | CHERRY_PICK_HEAD cleared; rebase reports `stopped_on_conflict` + re-run message | `cargo test --test resolve_integration cherry_pick_` / `rebase_stops_` (probe-gated) |
| 11 | Remote Resolved = one commit; Partial = none | resolution branch has exactly 1 new commit; Partial: 0 commits, no push with `--remote-push` | `cargo test --test resolve_remote partial_never_commits` etc. (fake `gh`/`glab` harness) |
| 12 | JSON purity + stderr preview | `--json --yes` stdout parses as one JSON object with new fields; interactive `--json` preview on stderr only | `cargo test --test resolve_integration json_` |
| 13 | `--dry-run` unchanged | no snapshot, no mutation, no stage, no finish | existing dry-run tests still pass unmodified |
| 14 | No-operation-ref conflict (`git checkout -m` style) | apply + stage succeed, finish reports `NothingToFinish`, staged-only headline | `cargo test --test resolve_integration nothing_to_finish` |
| 15 | External modification during confirm | file edited by "another terminal" between snapshot and rejection: not overwritten on restore, warning names it, other files restored | `cargo test --test resolve_integration restore_guard_external_edit` |
| 16 | Full gates | all green | `cargo fmt --check && cargo clippy -- -D warnings && cargo test` |

**Edge cases to verify**:
- Pre-existing staged-but-unrelated index content enters the finishing merge commit (document via an assertion - matches plain `git merge --continue` behavior).
- CRLF file and no-trailing-newline file survive snapshot/restore byte-exactly.
- `git checkout -m`-style conflict state (no operation ref): apply + stage succeed, finish reports `NothingToFinish`, staged-only headline.
- Re-run after a Partial: previously staged files are no longer unmerged; only the remainder is processed; finish fires when the last file is confirmed.
- Rebase whose remaining commits apply cleanly after the continue: classified `Completed`, not `StoppedOnNextConflict`.
- EOF at the second prompt of a multi-file run triggers the full abort-restore path (not a partial apply).
- SIGINT mid-confirm is the documented limitation (no restore); the README recovery command is verified to exist in the rewritten section.
- An Aborted remote run leaves the resolution branch commitless and the scratch dir cleaned up.
