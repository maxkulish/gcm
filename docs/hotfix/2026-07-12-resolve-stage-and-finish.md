# Resolve leaves the merge unfinished: make `gcm resolve` an ownership transaction

- Date: 2026-07-12 (v2, same day - revised after external review of v1)
- Status: **v2 accepted - all four open questions decided at owner review
  (2026-07-12, decisions section below); implementation tracked as CLO-555.**
  All 10
  external-review findings were validated against the code; verdicts with
  file:line evidence are in the appendix. The v1 configurable stage/finish
  matrix is withdrawn in favor of a strict transaction.
- Affected component: `src/resolve/mod.rs` (flow restructure), `src/ui.rs`
  (prompt parser), `src/git.rs` (finish helper, working-tree snapshot),
  `src/resolve/remote/mod.rs` (execution context + pre-existing Partial bug),
  `src/cli.rs` (`--no-finish`), README resolve section.
- Scope: behavior change to `gcm resolve` (local and, via shared engine, the
  remote wrapper's commit/push gating). `--dry-run` untouched. The `gcm`
  commit flow's `confirm()` adopts the same safe parser (decision 2).
- Decision: accepted with decisions 1-4 below.

## Problem

Field repro (2026-07-12, `~/Code/Vault/personal-health`): a merge with one
conflicted file. `gcm resolve` produced a correct LLM resolution, the user
pressed `Y`, gcm printed `All conflicts resolved.` - and `git status` still
showed `UU log.md` with `.git/MERGE_HEAD` present. Three manual commands
remained (`git add`, `git merge --continue`, `git push`). The tool exists to
resolve a conflict **and let the user continue working**; handing back a
checklist defeats it.

v1 of this document proposed staging on accept plus a three-mode
`finish = commit|stage|none` knob. External review found that the surrounding
runtime cannot honor the promise v1 made, and that the mode matrix itself
undermines it. The validation (appendix) confirmed every finding. v2 keeps
v1's end state and replaces the mechanism.

## The contract (v2)

> **Yes to every file = gcm applies everything, stages it, and finishes the
> Git operation. No to any file = gcm restores the repository to exactly the
> state it found and exits 0.**

One invariant, not a configuration surface. A single `--no-finish` flag
(CLI-only, no config key) exists as a debugging escape hatch: apply + stage
but skip the finishing commit.

## Design

### Three phases

**A - Propose (no lasting mutation).** Before touching anything, snapshot the
working-tree bytes of every unmerged file (in-memory map; precedent: the
commit flow's FR-47 index transaction, `snapshot_index`/`restore_index`,
`src/git.rs:215-223`). Then run today's pipeline - zdiff3 re-checkout,
mergiraf, classification, LLM, validation - to build one proposal per file.
zdiff3 and mergiraf still mutate the working tree in place (mergiraf is an
external tool and needs a real file), but under snapshot protection those
mutations are now reversible. Mergiraf successes become ordinary proposals -
they no longer bypass the preview (today they return `Accepted` unprompted,
`src/resolve/mod.rs:325-340`).

**B - Confirm (nothing applied yet).** Every proposal is previewed with a
`[y/N/e]` prompt. Decisions are collected; no file is written, staged, or
committed during this phase. Any `n` aborts the run: phase C is skipped and
the abort path runs instead. `e` opens `$EDITOR`; the edited text replaces the
proposal (after validation) and still waits for phase C.

**C - Apply (all-or-nothing).** Runs only when every file ended
Accepted/Edited. Write all proposals, stage all resolved paths **centrally by
final action** - including marker-free files that needed no write (they are
unmerged in the index today yet were never staged, `src/resolve/mod.rs:309-319`) -
then finish the operation and report the commit sha.

**Abort path.** Restore every snapshotted file's original bytes (the exact
pre-run content, including any manual partial resolution the user had made -
which today's zdiff3 re-checkout silently destroys), leave the index and
operation state untouched, exit 0. Headline states that nothing was changed.

### Escalation is not rejection

A file the *tool* cannot resolve (binary, sensitive-path, provider failure,
validation escalation) must not nuke the user's confirmed work, especially
under `--yes` in CI. So the transaction distinguishes:

- **User rejection** (`n`): whole-run abort + restore, exit 0.
- **Tool escalation**: apply and stage the files that were confirmed, do NOT
  finish, report Partial with an explicit list of still-unmerged paths and the
  exact next command.

`--yes` skips phase B (accepts all validated proposals) and then follows the
same rule: all resolved -> finish; any escalated -> stage progress, no finish.

### Prompt parser (`src/ui.rs`, shared by both prompts)

Today anything that is not exactly `n`/`N`/`e`/`E` selects Accept - `no`,
`No`, typos, and (worse) EOF/Ctrl-D, since `read_line` returning `Ok(0)`
leaves an empty string that falls to the `_ => Accept` arm
(`src/ui.rs:44-48`). That already contradicts the README's "EOF never
auto-accepts" claim (README.md:362); the start-of-run non-TTY guard
(`src/resolve/mod.rs:116-118`) does not cover a Ctrl-D mid-session.

New parser: case-insensitive `y`/`yes` -> Accept, `n`/`no` -> Reject,
`e`/`edit` -> Edit; **anything else reprompts** (bounded, e.g. 3 attempts then
Reject); **EOF and empty input mean Reject**. Prompt renders `[y/N/e]` -
default No, because Yes now carries commit authority.

The commit flow's `confirm()` has the identical anything-means-yes parser
(`src/ui.rs:89-100`) with the identical consequence (an unwanted commit), so
per decision 2 both prompts route through one shared parser function in this
hotfix - one parser, one behavior.

### Finish helper (`src/git.rs`)

`run_git` is unsuitable: it uses `.output()`, which nulls stdin and captures
stderr (`src/git.rs:95-110`) - pinentry cannot prompt and progress is
invisible. The commit flow already solved this: `commit_signed` hardcodes `-S`
and inherits stdin exactly so GPG/SSH pinentry works (`src/git.rs:230-241`).
The finish helper follows that pattern and preserves the signing contract:

- Merge: `git commit -S --no-edit` (consumes the prepared `MERGE_MSG`,
  creates the merge commit, clears `MERGE_HEAD`). Plain `git merge --continue`
  is NOT used - it only signs if git config happens to demand it.
- Rebase / cherry-pick: `git -c commit.gpgsign=true rebase --continue` /
  `cherry-pick --continue`, with `GIT_EDITOR=true` in the child env.
- Stdin/stderr inherited; hooks run normally (`--no-verify` stays exclusive to
  the remote scratch repo, whose hooks belong to a throwaway clone).
- Outcome classified by **postconditions**, not stderr text: operation ref
  gone + no unmerged entries + HEAD moved -> `Completed { head_sha }`;
  operation ref present + new unmerged entries -> `StoppedOnNextConflict`;
  anything else -> `Failed` (staged state preserved, hook stderr already on
  the terminal, exit non-zero with the manual continue command named).

### Remote execution context

Local and remote share `run_resolve_in_repo`
(`src/resolve/remote/mod.rs:81,90`), so "remote untouched" was false in v1:
finish logic inside the engine would commit the scratch merge, after which the
wrapper's `commit_resolution` (`:99`, `git add -A` + `--allow-empty`,
`:162-174`) would stack a second, empty commit.

The engine therefore takes an explicit context: **local** finishes; **remote**
never does - the wrapper remains the sole committer in the scratch repo.

Two pre-existing remote bugs surfaced by this review are fixed in the same
change because the gating is one condition:

- `commit_resolution` runs unconditionally, so a **Partial** report (skipped /
  escalated files) is committed with raw conflict markers staged by
  `git add -A`, and `--remote-push` (`:107-110`) will push that branch. Fix:
  commit and push only when status is Resolved or Noop (the clean-merge case
  keeps `--allow-empty`); Partial reports the scratch path and stops.

### Push boundary

- Local `gcm resolve`: **never pushes.** Completion means the merge / rebase
  step / cherry-pick is committed locally. (The field repro's `git push` is
  the one step that stays manual, deliberately.)
- Remote `--pr`/`--mr`: pushing remains gated on the explicit `--remote-push`
  flag, now additionally gated on a non-Partial report (above).

### Rebase scope (explicit, not implied)

This hotfix completes **one conflict stop**. For a multi-commit rebase where
the next commit also conflicts, the finish helper reports
`StoppedOnNextConflict` and the headline says: `rebase continued and stopped
on the next conflicted commit - run 'gcm resolve' again.` The full
resolve-until-clean loop (fresh confirmation and provider-cost boundary per
round) is filed as CLO-554.

### Output

Headlines always match the `git status` a user would see next:

- Completed: `All conflicts resolved - merge committed (abc1234).` (op named
  per state: merge/rebase step/cherry-pick)
- `--no-finish` or hook failure: `Resolutions staged. Run 'git merge
  --continue' to finish.`
- User rejection: `Aborted - working tree restored, nothing changed.`
- Escalation: current Partial line + explicit unmerged list + next command.

JSON (`ResolveReport`) gains additive fields:
`"staged": [paths]`, `"finish": {"result": "completed" | "stopped_on_conflict"
| "failed" | "skipped", "commit": "abc1234"?}`, `"restored": bool`.

`--json` without `--yes` currently hides the preview but still prompts
(`src/resolve/mod.rs:546` passes `args.json` as `quiet`; `src/ui.rs:32-36`) -
on a TTY the user confirms blind. With Yes now meaning "commit", that is
untenable: `--json` interactive prompts print the preview to **stderr**
(stdout stays a pure JSON envelope), or the user passes `--yes`.

### README impact

Rewrite the resolve pipeline step 5 and the safety-guarantees paragraph
(README.md:345-364) around the transaction: "gcm resolve either completes the
operation (apply + stage + signed finishing commit) after you confirm every
file, or - on any No - restores the repository byte-for-byte and exits 0.
Files the tool escalates are staged if confirmed, never finished silently.
EOF and unrecognized input never accept." Update the intro line (README.md:19)
and document `--no-finish`. Drop the now-false "never runs `git add` or
`--continue`" and "EOF never auto-accepts" claims.

### Cleanup while in the area

The mergiraf block's inner `if args.dry_run` arm (`src/resolve/mod.rs:326-330`)
is dead - the enclosing condition already requires `!args.dry_run`. Remove it.

## Test plan

Prompt parser (unit, `src/ui.rs`):
- `No`, `no`, `NO` -> Reject (today: Accept).
- `y`, `Y`, `yes` -> Accept; `e`, `edit` -> Edit.
- Unknown input (`ok`, `q`) reprompts, then defaults Reject after the bound.
- EOF / empty line -> Reject.
- Same cases against the commit flow's `confirm()` (shared parser): `no` and
  EOF abort instead of committing.

Transaction (integration, temp repos):
- Reject on file 2 of 2 restores file 1's exact pre-run bytes (byte-compare),
  index untouched, MERGE_HEAD intact, exit 0.
- Pre-run manual partial edits to a conflicted file survive an abort.
- All-accept merge: files staged, MERGE_HEAD gone, HEAD is a 2-parent commit,
  sha in headline and JSON.
- Marker-free unmerged file (prior manual resolve) gets staged in phase C.
- Mergiraf-resolved file is previewed and confirmed, not auto-accepted; its
  rejection restores markers.
- Escalated + accepted mix: accepted staged, no finish, Partial lists the
  remainder; same under `--yes`.
- `--no-finish`: staged, MERGE_HEAD intact, hint printed.
- Hook failure (pre-commit exiting 1): staged state intact, exit non-zero.
- Signed merge commit (test signing key): `git log --show-signature` verifies;
  same for cherry-pick completion.
- Cherry-pick completion clears CHERRY_PICK_HEAD; single-conflict rebase
  completes; two-conflict rebase reports `stopped_on_conflict`.
- Pre-existing staged-but-unrelated index content: document (assert) that it
  enters the finishing commit, matching plain `git merge --continue` behavior.
- `--dry-run` mutates nothing, snapshots nothing (unchanged).

Remote (integration):
- Resolved path produces exactly one commit on the resolution branch (no
  empty duplicate).
- Partial report: no commit, no push even with `--remote-push`; scratch path
  reported.

JSON:
- New fields present; stdout parses as a single JSON object in `--json --yes`;
  interactive `--json` preview goes to stderr only.

## Acceptance criteria

- [ ] Yes to every file leaves: clean index, operation completed by a signed
  commit, zero manual steps before `git push`.
- [ ] Any No leaves the repository byte-identical to the pre-run state, exit 0.
- [ ] `no`/`No`/typos/EOF can never accept; unknown input reprompts.
- [ ] Mergiraf and marker-free files flow through the same confirm + stage
  path as LLM files.
- [ ] Tool escalation stages confirmed work, never finishes, and names the
  remaining paths and next command.
- [ ] Remote wrapper: exactly one commit, never on Partial, push gated
  likewise; local resolve never pushes.
- [ ] Every headline is consistent with subsequent `git status` output.
- [ ] README guarantees match the runtime exactly.
- [ ] `cargo test` / `clippy` / `fmt` green.

## Decisions (owner review, 2026-07-12)

1. **Escalation stages progress and stops** - a tool escalation applies and
   stages the confirmed files, skips the finish, and reports Partial with the
   remaining paths. Full abort is reserved for user rejection. Rationale:
   a strict abort would void `--yes` runs whenever a single binary file
   appears.
2. **Commit-flow parser aligned in this hotfix** - `confirm()` and
   `confirm_file()` share one parser (`[y/N/e]`, reprompt on unknown, EOF =
   No). One parser, one behavior.
3. **Remote Partial commit/push fix rides along** - it is one gating
   condition in code this hotfix already restructures, and the marker-push
   hazard is live.
4. **Rebase resolve-until-clean loop filed as CLO-554** (Backlog, Feature) -
   this hotfix completes one conflict stop; the loop builds on
   `StoppedOnNextConflict` later.

## Appendix: v1 review findings validated against the code

| # | Finding | Verdict | Evidence |
|---|---|---|---|
| 1 | Only exact `n`/`N` rejects; `No`/`no`/other input accepts | **Confirmed, plus worse**: EOF/Ctrl-D also accepts (`read_line` -> `Ok(0)` -> empty -> `_ => Accept`), contradicting README.md:362; the non-TTY guard (`src/resolve/mod.rs:116`) only covers run start | `src/ui.rs:44-48` |
| 2 | `n` skips per-file and continues; accepted files stay modified (staged, under v1) | **Confirmed**; loop continues at `src/resolve/mod.rs:123`, Skip arm `:551`, Partial bookkeeping `:154-159`. Run-level ownership adopted | `src/resolve/mod.rs:123,551` |
| 3 | Files mutate before confirmation; v1's README guarantee was false | **Confirmed**: zdiff3 re-checkout `:103-106` (also destroys manual partial resolutions - pre-existing data-loss hazard), mergiraf in-place `src/resolve/mergiraf.rs:48-52`, mergiraf auto-Accepted with no prompt `:325-340` | `src/resolve/mod.rs:103,325` |
| 4 | Staging beside `write_file` misses early-Accepted paths | **Confirmed**: marker-free `:309-319` and mergiraf `:325-340` return Accepted without any write; v1 would report Resolved then fail to finish | `src/resolve/mod.rs:309,325` |
| 5 | Remote shares the engine; v1 "remote untouched" false; double/empty commit risk; Partial commits and can push marker content | **Confirmed** on all counts: shared engine `:81,90`, unconditional `commit_resolution` `:99` with `add -A` `:162` + `--allow-empty` `:174`, push not status-gated `:107-110` | `src/resolve/remote/mod.rs` |
| 6 | Three finish modes make Yes configuration-dependent | **Agreed (design)**: modes withdrawn; single invariant + `--no-finish` escape hatch, no config key | - |
| 7 | Push boundary undefined | **Partially valid**: v1 never proposed local push but never said so; now explicit (local never; remote keeps `--remote-push`) | - |
| 8 | Rebase completes one stop only | **Partially valid**: v1's edge-case section said this; now elevated into the contract, headline, and JSON (`stopped_on_conflict`) | v1 doc |
| 9 | Signing + subprocess behavior underspecified | **Confirmed**: `commit_signed` hardcodes `-S`, inherits stdin for pinentry `src/git.rs:230-241`; `run_git` nulls stdin / captures stderr `src/git.rs:95-110`; postcondition classification adopted | `src/git.rs:95,230` |
| 10 | Test plan gaps, incl. `--json` without `--yes` prompting blind | **Confirmed**: `quiet = args.json` suppresses the preview but the prompt still fires (`src/resolve/mod.rs:546`, `src/ui.rs:32-36`); all listed tests adopted | `src/ui.rs:32` |
