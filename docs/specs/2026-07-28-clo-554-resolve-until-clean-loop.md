# Spec: Resolve-until-clean rebase loop for `gcm resolve`

**Task**: [CLO-554](https://linear.app/cloud-ai/issue/CLO-554)
**Created**: 2026-07-28
**Estimated scope**: M (9 files, 8 sub-tasks)

Owner decisions (do not relitigate): (1) a **round gate** asks before spending
provider calls on each round after the first - the per-file confirmations alone
are not the cost boundary; (2) the round cap defaults to **10**; (3) the loop
keys on the `StoppedOnNextConflict` signal, so cherry-pick sequences get the
same treatment as rebases for free; (4) the loop driver lives in `run_resolve`,
not in `run_resolve_in_repo`, so the remote wrapper is untouched.

## 1. Problem Statement

`gcm resolve` completes exactly one conflict stop. CLO-555 turned it into a
three-phase transaction - propose, confirm-all, apply - whose apply phase ends
by calling `Repo::finish_conflict_op` (`src/git.rs:282`). That helper classifies
the outcome by postconditions and returns
`FinishOutcome::StoppedOnNextConflict` (`src/git.rs:24`, produced at
`src/git.rs:329-333`) when a rebase or cherry-pick continued past the current
stop and halted on the next conflicted commit in its sequence.

Today that outcome is terminal. `run_resolve_in_repo` maps it to
`FinishResult::StoppedOnConflict` (`src/resolve/mod.rs:446-450`) and
`print_human_report` prints "the rebase continued and stopped on the next
conflicted commit. Run 'gcm resolve' again." (`src/resolve/mod.rs:1044-1049`).
The user reruns the whole command per conflicted commit: an N-conflict rebase
costs N invocations.

What the loop actually saves is process re-invocation and re-prompting - the
user staying in one flow instead of re-entering the command N times. It does not
save the per-round setup: `config::load()` + `apply_to_env`
(`src/resolve/mod.rs:179-181`), `provider::select` (`:209`), and
`resolve_conflict_config`'s own `config::load()` (`:546`) all still run once per
round, and the driver's own `max_rounds` read adds one more config read. None of
those touch the network (`provider::select` is pure construction), so the cost is
process-local.

This was deliberate. `docs/hotfix/2026-07-12-resolve-stage-and-finish.md:160-166`
scoped the hotfix to one stop and filed the loop as CLO-554; the CLO-555 spec
records the same exclusion (`docs/specs/2026-07-12-clo-555-resolve-ownership-transaction.md:60`).
The existing integration test `rebase_stops_on_next_conflict_reports_rerun`
(`tests/resolve_integration.rs:1116-1172`) pins today's single-stop behavior and
names CLO-554 as the owner of looping.

Who is affected: anyone rebasing a feature branch across a moved mainline, which
is the tool's core use case.

The reason this is not simply "call the engine in a `while` loop" is provider
spend. LLM calls happen in the propose phase (`src/resolve/mod.rs:239-281`),
*before* any confirmation. A naive loop would spend on round 2 before the user
learns round 2 exists. The engine must therefore gain a boundary the user sees
and controls between rounds, plus a hard cap so an unattended `--yes` run cannot
burn an unbounded number of calls.

One pre-existing hazard the loop widens rather than creates: gcm installs no
signal handler anywhere, so a Ctrl-C during a round's propose phase leaves that
round's zdiff3 re-checkout in place, unrestored. Under the loop this window
repeats per round and sits in front of already-committed rounds. Recovery is
unchanged - re-run `gcm resolve`, which re-reads the unmerged set from git - and
handling signals is out of scope here.

## 2. Acceptance Criteria

- [ ] **AC1 - Multi-round completion.** A rebase of 3 commits where 2 conflict
      completes in a single `gcm resolve` invocation when the user accepts each
      round: the rebase is finished (no `REBASE_HEAD`, no unmerged entries), each
      step is committed, and the headline names the total round count.
- [ ] **AC2 - Round gate bounds spend.** After a round finishes with
      `StoppedOnConflict`, gcm prints the next stop's commit sha and conflicted
      file count and asks `[y/N]` **before** any provider call for that round. A
      No exits the loop cleanly (exit 0) with **zero** provider requests issued
      for the declined round, proven by a request counter on the mock provider,
      not by inference. Under `--yes` the gate does not prompt and does not block.
- [ ] **AC3 - Round cap.** `--max-rounds <N>` (CLI) and `[conflict].max_rounds`
      (config) bound the number of rounds, defaulting to 10, CLI winning over
      config. `--max-rounds 0` is rejected by clap at parse time, before repo
      discovery or any provider call; `[conflict].max_rounds = 0` is rejected as a
      config error with the same actionable message. On hitting the cap the loop
      stops and the message names the remaining state and both ways forward.
- [ ] **AC4 - `--max-rounds 1` preserves legacy behavior.** With `--max-rounds 1`
      a rebase that stops again leaves **identical git state** (step committed,
      `REBASE_HEAD` set, next commit's conflicts unmerged), the **same exit code**
      (0), and a headline still containing "stopped on the next conflicted
      commit". It *additionally* emits the `loop` object with
      `terminal == "cap_reached"` and `rounds_run == 1` - passing the flag is an
      explicit opt-in to loop metadata, so this is not a compatibility break.
- [ ] **AC5 - No mid-loop data loss.** A No to a file proposal in round k
      restores round k byte-for-byte (existing `WorkingTreeSnapshot` contract),
      leaves rounds 1..k-1 committed, leaves the operation stopped, and exits 0.
      The same holds for a No at the round gate. No exit path leaves a
      half-applied round. When rounds are already committed, the abort headline
      must **not** say "nothing changed" (today's text at
      `src/resolve/mod.rs:1064`); it must state that round k was restored and
      that rounds 1..k-1 remain committed.
- [ ] **AC6 - Per-round visibility.** Every round prints, to stderr, a banner
      before proposing (`round i (cap N)`, the commit being applied, conflicted
      file count) and a summary after finishing (hunks total / auto / LLM /
      escalated, and the step commit sha). The banner says `cap N`, never
      "round i of N" - N is the ceiling, not the number of rounds the operation
      needs. In `--json` mode these stay on stderr and stdout remains exactly one
      JSON object.
- [ ] **AC7 - JSON report.** The report gains an additive `loop` object carrying
      `rounds_run`, `max_rounds`, `terminal`, and a `rounds` array with one entry
      per round (`round`, `commit`, `status`, `files`, `staged`, `finish`).
      `SCHEMA_VERSION` stays 1. For any run where the user did not pass
      `--max-rounds` and the loop did not engage, `loop` is absent and the
      envelope is byte-identical to the pre-CLO-554 output.
- [ ] **AC8 - Envelope on non-completed terminals.** Top-level `files`, `staged`,
      and `finish` always describe the **last round attempted**. Concretely: on a
      mid-loop file rejection the top-level `status` is `aborted`, `restored` is
      true, and `finish` is **absent** (the Skip arm at
      `src/resolve/mod.rs:348-376` returns before the finish step) - consumers
      must read `loop.rounds[].finish` for the committed rounds. On a declined
      gate the top-level `status` is `resolved` with
      `finish.result == "stopped_on_conflict"` (the round that ran did resolve)
      and `loop.terminal == "declined"`.
- [ ] **AC9 - Escalation stops the loop.** An escalated file in round k stages
      that round's confirmed work, skips the finish, reports `partial`, and does
      not start round k+1 - under `--yes` as well as interactively.
- [ ] **AC10 - Single-round modes.** `--dry-run` and `--no-finish` each execute
      exactly one round, never reach the round gate, and omit `loop` from the JSON
      report. `--dry-run` previews the first stop only and says so.
- [ ] **AC11 - No-progress guard.** If the operation head sha does not advance
      between rounds, or is absent while the finish reported `StoppedOnConflict`,
      the loop stops with `loop.terminal == "no_progress"`, a warning diagnostic
      on stderr naming the stalled commit, and exit 0 - never a second proposal
      for the same commit.
- [ ] **AC12 - Output matches `git status`.** After every exit path (completed,
      cap reached, gate declined, file rejected, escalated, no-progress) the
      headline and the JSON agree with what `git status` reports, and any
      still-stopped operation names the exact commands out - including that
      `git rebase --abort` discards the rounds already committed.

**Verification method**: `make check` (fmt-check + clippy + full test suite) for
the unit and integration layer; the tests in §5 for AC1-AC12 behaviorally;
`scripts/acceptance.sh` for the PTY-driven gate case; manual `git status`
comparison after each test's exit path.

## 3. Constraints

**Must**:

- Put the loop driver in `run_resolve` (`src/resolve/mod.rs:128`), calling
  `run_resolve_in_repo` once per round. `run_resolve_in_repo` stays a
  single-round function.
- Key the continue decision on `FinishResult::StoppedOnConflict` in the returned
  report, not on the operation name, so rebase and cherry-pick sequences both
  loop. `Completed`, `Skipped`, and the `FinishFailed` error keep today's
  semantics exactly.
- Re-read the operation head sha **immediately after the gate is accepted**, not
  before the prompt is shown. A user parked at a `[y/N]` gate can mutate the repo
  from another terminal; the sha that feeds the round banner and the no-progress
  comparison must be the post-gate read, so out-of-band changes are seen rather
  than papered over by a stale value.
- Reuse the CLO-555 prompt contract for the round gate: explicit `y`/`yes` only;
  bare Enter, EOF, `no`, and unrecognized input all mean No; bounded reprompt via
  the existing `PROMPT_ATTEMPTS` budget (`src/ui.rs:28`). The gate must share
  that code path, not re-implement parsing.
- Reject `--max-rounds 0` with clap's `value_parser!(u32).range(1..)` so it fails
  at argument-parse time. `resolve_conflict_config` (`src/resolve/mod.rs:508`)
  returns `ConflictConfig`, not a `Result`, and runs after repo discovery - it is
  not a place an error can be raised. A config-file `max_rounds = 0` must be
  rejected as a `GcmError::Config` at load, not silently clamped.
- Print all round banners, summaries, and the gate prompt to **stderr**. Stdout
  carries the final human headline, or exactly one JSON object under `--json`.
- Mirror the new `max_rounds` config field in **both** the serde default and the
  hand-written `impl Default for ConflictConfig` (`src/config.rs:126-136`) - the
  CLO-555 review found a live bug from these drifting apart.
- Carry a no-progress guard as a pure, unit-testable predicate over
  (previous op head, current op head), so it can be tested without provoking a
  stalled rebase.
- Keep every loop terminal at exit 0. Only a genuine failure
  (`GcmError::FinishFailed` and friends) exits non-zero, as today.
- Document `max_rounds` in the README `[conflict]` table and its sample TOML
  (`README.md:395-415`). Nothing in the compiler forces this edit the way the
  `ConflictConfig` destructure does.

**Must-not**:

- Do not move looping into `run_resolve_in_repo` or otherwise change the remote
  path (`src/resolve/remote/`). A remote resolve works on a merge in a scratch
  repo and never produces `StoppedOnNextConflict`.
- Do not prompt at the round gate under `--yes`. A non-TTY run without `--yes`
  (or `--no-input`) is already rejected by `needs_terminal_but_absent`
  (`src/ui.rs:203`, ADR-001 #10) before round 1 begins, so the gate can never
  face an unanswerable prompt - do not add a second guard, and do not weaken the
  first.
- Do not emit a JSON object per round, and do not bump `SCHEMA_VERSION`. The
  `loop` field is additive, following the `staged`/`finish`/`restored` precedent
  (`src/resolve/report.rs:11-30`).
- Do not carry a Yes across rounds. Each round runs its own full
  propose/confirm/apply transaction; round k's per-file confirmations authorize
  round k only.
- Do not treat an escalation as a reason to continue. Escalated work means the
  finish was skipped, so there is no new stop to loop onto.
- Do not loop in `--dry-run`. Dry-run performs no finish, so it previews the
  first stop only; say so rather than pretending to know future conflicts.

**Prefer**:

- Reuse `FileReport`/`FinishReport` verbatim inside each `RoundReport` rather
  than inventing parallel shapes.
- Keep the top-level `files`, `staged`, and `finish` fields meaning "the last
  round attempted", so a single-round run is unchanged for existing consumers.
- Name commits in output by short sha plus subject line where cheap; short sha
  alone is acceptable.
- Note in the README that with commit signing enabled, each round's
  `git <op> --continue` may trigger a passphrase prompt or a hardware-token
  touch - expected, but worth knowing before starting a 10-round loop.

**Escalate when**:

- The no-progress guard fires in a test that is not deliberately provoking it -
  that means the postcondition classification is wrong, which is CLO-555
  territory, not this task's.
- Making the gate work would require changing `WorkingTreeSnapshot` restore
  semantics. The restore contract is settled; a loop that needs to weaken it is
  the wrong loop.

## 4. Decomposition

1. **Operation-head helper** - add `Repo::conflict_op_head() -> Option<(&'static str, String)>`
   returning the operation name and short sha of `REBASE_HEAD` or
   `CHERRY_PICK_HEAD` (dispatch order matching `finish_conflict_op`: rebase
   before cherry-pick before merge). Unit tests for rebase-stopped, cherry-pick
   stopped, merge (None), and clean tree (None) - files: `src/git.rs`.

2. **Cap plumbing** - add `max_rounds: Option<u32>` to `Commands::Resolve` with
   `value_parser!(u32).range(1..)`, a `Cli::max_rounds()` accessor mirroring
   `Cli::no_finish()` (`src/cli.rs:184-193`), a `max_rounds` field on
   `ConflictConfig` defaulting to 10 in **both** the serde default and the
   `Default` impl, a `GcmError::Config` rejection for a configured `0`, and
   CLI-over-config precedence in `resolve_conflict_config`
   (`src/resolve/mod.rs:508`) - files: `src/cli.rs`, `src/config.rs`,
   `src/resolve/mod.rs`.

3. **Report shape** - add `LoopReport { rounds_run, max_rounds, terminal, rounds }`,
   `RoundReport { round, commit, status, files, staged, finish }`, and
   `LoopTerminal { Completed, CapReached, Declined, Aborted, Partial, NoProgress }`
   (snake_case). Attach as `#[serde(rename = "loop", skip_serializing_if = "Option::is_none")]`.
   Omission rule: `loop` is present iff the loop driver engaged - more than one
   round ran, or the run stopped at the loop's own boundary (`CapReached` or
   `Declined`). Every scenario that predates CLO-554 and passes no `--max-rounds`
   therefore omits it. Serde tests for both the omitted and populated shapes -
   files: `src/resolve/report.rs`.

4. **Round gate prompt** - add a yes/no prompt helper in `src/ui.rs` built on the
   existing `prompt_choice_from` machinery: `y`/`yes` true; `n`/`no`/empty/EOF
   false; anything else reprompts within `PROMPT_ATTEMPTS` then resolves false.
   Unit tests over a `BufRead` fixture covering Enter, EOF, `e`, and garbage -
   files: `src/ui.rs`.

5. **Loop driver** - `run_resolve` becomes: round loop calling
   `run_resolve_in_repo(repo, args, Local)`, accumulating `RoundReport`s,
   printing the stderr banner before each round and the summary after, consulting
   the cap and the gate between rounds, re-reading the op head **after** the gate
   is accepted, and feeding that read into a pure `fn advanced(prev, now) -> bool`
   guard. Assembles the final `ResolveReport` with `files`/`staged`/`finish` from
   the last round attempted. Depends on 1-4 - files: `src/resolve/mod.rs`.

6. **Terminal output** - extend `print_human_report` for the loop terminals:
   completed-in-N-rounds, cap reached, gate declined, mid-loop abort, mid-loop
   partial, no-progress. Replace the flat "Aborted - working tree restored,
   nothing changed" for the mid-loop case (`src/resolve/mod.rs:1064`) with text
   that names the restored round and the committed ones. Each still-stopped
   terminal names re-running `gcm resolve`, the by-hand path (`git add` then
   `git <op> --continue`), and `git <op> --abort` with an explicit note that
   abort discards the rounds already committed. Depends on 3 and 5 - files:
   `src/resolve/mod.rs`.

7. **Mock-provider harness** - `mock_ollama_server_multiple`
   (`tests/resolve_integration.rs:115-151`) cannot currently prove a request was
   *not* made: it has no request counter, and its 10s accept budget is computed
   once outside the response loop. Add an `Arc<AtomicUsize>` request counter
   returned to the test, and make the join bounded and non-hanging when fewer
   requests arrive than responses queued. Re-check the budget against the PTY
   acceptance case, which is slower than the unit-level ones. AC2 is unverifiable
   until this lands - files: `tests/resolve_integration.rs`.

8. **Tests** - the cases in §5 plus one acceptance case for the interactive gate;
   update `rebase_stops_on_next_conflict_reports_rerun` to pass `--max-rounds 1`
   so it keeps pinning single-stop behavior deliberately rather than by omission,
   and assert both the legacy git state and the new `cap_reached` metadata.
   Depends on 5, 6, 7 - files: `tests/resolve_integration.rs`,
   `scripts/acceptance.sh`, `README.md`.

**Dependency order**: 1, 2, 3, 4, 7 are independent and can land in any order or
in parallel. 5 requires 1-4. 6 requires 3 and 5. 8 requires 5, 6, and 7.

## 5. Evaluation

Two harness facts shape the table below, both discovered during implementation:

1. **Unit tests run via `cargo test --bin gcm`**, not `--lib` - gcm is a binary
   crate with no library target (that is what CLO-595 is about).
2. **Interactive cases cannot be integration tests.** `needs_terminal_but_absent`
   (`src/ui.rs:203`, ADR-001 #10) rejects a non-TTY run without `--yes`, and the
   integration harness pipes stdin. So the gate is proven at two levels instead:
   the prompt contract by unit test, the end-to-end behavior by the PTY
   acceptance script - the same split CLO-555 used for AC-R1/AC-R2. AC2's
   zero-spend guarantee is *also* proven hermetically on the cap boundary, where
   the request counter shows the capped-off round issued nothing.

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | 3-commit rebase, 2 conflicting commits, `--yes`, 2 queued responses | Single invocation finishes the rebase: no sequencer dir, no unmerged entries, both steps committed, `loop.terminal == "completed"`, `rounds_run == 2`, the two rounds name different commits, counter reads 2 | `cargo test --test resolve_integration rebase_loop_completes_multi_commit_in_one_invocation` |
| 2 | Same rebase, `--max-rounds 1 --yes`, 2 queued responses | Legacy half of AC4: headline contains "stopped on the next conflicted commit", rebase still stopped, next conflict unmerged, exit 0. Counter reads **1** - the capped-off round spent nothing | `cargo test --test resolve_integration rebase_stops_on_next_conflict_reports_rerun` (updated legacy pin) |
| 3 | Same, with `--json` | New half of AC4: `loop.terminal == "cap_reached"`, `rounds_run == 1`, `max_rounds == 1`, `stopped_on` names the stop. (Separate run because `--json` suppresses the human headline test 2 asserts.) | `cargo test --test resolve_integration max_rounds_one_emits_cap_reached_metadata` |
| 4 | `--max-rounds 0` | clap rejects at parse time, non-zero exit, conflicted file byte-identical afterwards | `cargo test --test resolve_integration max_rounds_zero_is_rejected_at_parse_time` |
| 5 | `[conflict] max_rounds = 0` in config, no CLI flag | Non-zero exit, stderr contains "max_rounds must be at least 1", no mutation | `cargo test --test resolve_integration config_max_rounds_zero_is_rejected_before_mutation` |
| 6 | `--max-rounds 1` with `[conflict] max_rounds = 5` | CLI wins: `loop.max_rounds == 1`, `terminal == "cap_reached"`, counter reads 1 | `cargo test --test resolve_integration max_rounds_flag_beats_config` |
| 7 | `git cherry-pick base..feature`, 2 conflicting commits, `--yes` | Loops on the same signal with no cherry-pick-specific code; `rounds[0].finish.op == "cherry-pick"`; `rounds_run >= 2` | `cargo test --test resolve_integration cherry_pick_sequence_loops_on_the_same_signal` |
| 8 | Rebase where round 2's provider response is unusable, `--yes` | Loop stops at round 2: `status == "partial"`, `loop.terminal == "partial"`, `rounds_run == 2`, round 1 still in the log, rebase left stopped, no third request | `cargo test --test resolve_integration rebase_loop_escalation_stops_loop_and_keeps_earlier_rounds` |
| 9 | Multi-commit rebase with `--no-finish` | Exactly one round (counter reads 1), no `loop` key in the JSON | `cargo test --test resolve_integration no_finish_runs_a_single_round_without_loop_metadata` |
| 10 | Multi-commit rebase with `--dry-run` | No `loop` key, rebase untouched | `cargo test --test resolve_integration dry_run_runs_a_single_round_without_loop_metadata` |
| 11 | No-progress predicate over (prev head, now head) | Advancing sha -> true; identical sha -> false; vanished head -> false; first head -> true | `cargo test --bin gcm resolve::tests::advanced` |
| 12 | Single-file merge conflict, `--yes` (no looping possible) | Pre-CLO-554 envelope unchanged | `cargo test --test resolve_integration transaction_yes_merge_finishes_signed` |
| 13 | Report serialization: `loop` omitted vs populated | Omitted on a plain single round; populated with `rounds`/`terminal`/`rounds_run`/`max_rounds`; wire name is bare `loop`, never `loop_report` | `cargo test --bin gcm resolve::report` |
| 14 | Round-gate prompt parser | `y`/`yes` accept; Enter, EOF, `no`, `e`, garbage all reject; reprompt bounded at 3 | `cargo test --bin gcm ui::tests::round_gate` |
| 15 | Operation-head helper | Correct short sha under a stopped rebase and a stopped cherry-pick; `None` under a merge and a clean tree | `cargo test --bin gcm git::tests::conflict_op_head` |
| 16 | **Completed-rebase classification (regression)** | A `rebase --continue` that finishes the sequence reports `Completed`, and `is_rebasing()` is false afterwards despite `REBASE_HEAD` lingering | `cargo test --bin gcm git::tests::finish_rebase_completing_the_sequence_reports_completed` |
| 17 | Round gate declined, under a PTY | Exit 0; gate names round 2 before spending; "at your request" headline; round 1 still committed; rebase left stopped; recovery names `git rebase --abort` | `scripts/acceptance.sh` AC-R3 (skips without `expect` or signing) |
| 18 | Every gate accepted, under a PTY | Exit 0; rebase completed in one invocation; nothing unmerged; headline says "across 2 rounds" | `scripts/acceptance.sh` AC-R4 |
| 19 | Full suite + lints | Green | `make check` |

### Bug found while implementing this (in scope, fixed here)

`Repo::is_rebasing` tested `REBASE_HEAD`, but **git leaves that ref behind after
a rebase completes**. `has_conflict_state()` therefore stayed true on a finished
rebase and `finish_conflict_op` classified the success as
`FinishOutcome::Failed`, surfacing as a `FinishFailed` error with exit 1.

It stayed latent because nothing drove a rebase to completion through gcm before:
CLO-555 always ended while the ref was legitimately live. The loop is the first
code path that finishes a sequence, so it fails on round 2 of every multi-commit
rebase until this is fixed. Detection now uses git's own signal - the
`rebase-merge` / `rebase-apply` sequencer directory, resolved through
`rev-parse --git-path` so linked worktrees work. Pinned by test 16.

**Fixture invariant for tests 1, 2, 3, 6**: the 3-commit rebase fixture must have
exactly one conflicted file with one non-trivial (Complex) hunk per conflicting
commit, so one queued mock response maps to exactly one round. Stated in a
comment at `two_conflict_rebase` - otherwise a later tweak silently changes what
the request counter proves.

**PTY fixture invariant (tests 17, 18)**: the acceptance mock serves one canned
resolution for *every* request, so the two conflicting commits must touch
**different files**. If both rounds resolved the same file, round 2's patch would
come out identical to HEAD and git would stop on "the previous cherry-pick is now
empty" instead of completing.

**Edge cases to verify**:

- **Cherry-pick sequence** (test 7): the loop must not be rebase-specific - this
  is the whole reason for keying on the signal rather than the op name.
- **Remote (`--pr`/`--mr`)**: unchanged - the wrapper calls
  `run_resolve_in_repo` directly and never enters the loop driver.
- **Repo mutated while parked at the gate**: the post-gate head re-read is the
  one that counts; a changed sha flows into the banner and the guard normally.
- **Signing unavailable**: the loop tests follow the existing
  `signing_available()` skip guard (`tests/resolve_integration.rs:780`); the
  finish path requires real signing.
- **Signing prompts per round**: with SSH/GPG signing enabled, each round's
  `git <op> --continue` may raise pinentry or a token touch. Expected, documented
  in the README, not suppressed.
- **Cap reached with work remaining**: message names both `gcm resolve` (to
  continue) and `--max-rounds` (to raise the bound), and states that
  `git rebase --abort` would discard the rounds already committed.
- **External edit mid-loop**: the existing restore guard warns and skips rather
  than clobbering (`src/resolve/mod.rs:68-83`); a mid-loop abort inherits that
  behavior unchanged.
- **Ctrl-C mid-round**: no signal handling exists; the interrupted round's
  zdiff3 re-checkout stays on disk and `gcm resolve` re-run is the recovery path.
  Out of scope, documented in §1.
