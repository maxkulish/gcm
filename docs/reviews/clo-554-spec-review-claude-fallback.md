# Spec Review: clo-554

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-07-28
**Pipeline**: lok spec-review
**Note**: Both external reviewers failed; this is the fallback review

---

I read the spec and verified every code reference it makes against the actual source. Findings below.

# Review: CLO-554 resolve-until-clean loop

**Spec**: `docs/specs/2026-07-28-clo-554-resolve-until-clean-loop.md`
**Verified against**: `src/git.rs`, `src/resolve/mod.rs`, `src/resolve/report.rs`, `src/ui.rs`, `src/cli.rs`, `src/config.rs`, `src/provider/mod.rs`, `tests/resolve_integration.rs`, `scripts/acceptance.sh`, `README.md`

## 1. Problem Statement

Accurate and well-sourced. Every citation checks out: `FinishOutcome::StoppedOnNextConflict` at `src/git.rs:24` produced at `src/git.rs:329-333`, mapped to `FinishResult::StoppedOnConflict` at `src/resolve/mod.rs:446-450`, printed at `src/resolve/mod.rs:1044-1049`. The legacy pin test at `tests/resolve_integration.rs:1116` exists and does name CLO-554.

The framing of *why this isn't a `while` loop* — provider spend happens in propose (`src/resolve/mod.rs:239-281`), before any confirmation — is the correct core insight and justifies the round gate cleanly.

One overstatement (L1): §1 lists the per-invocation cost as "re-discovering the repo, re-loading config, re-selecting the provider, and re-prompting from scratch." The chosen seam hoists only repo discovery. `config::load()` + `apply_to_env` (`src/resolve/mod.rs:179-181`) and `provider::select` (`:214`) still run per round, and `resolve_conflict_config` does its own `config::load()` (`:545`), so the driver reading `max_rounds` adds an N+1th disk read. `provider::select` is pure construction (`src/provider/mod.rs:433-462`), so no hidden network cost — but trim the claim to what the loop actually saves: process re-invocation and re-prompting.

## 2. Acceptance Criteria

AC1, AC2, AC4, AC7 are testable and correctly scoped. AC7 in particular is *free* given the existing code, and the spec is right about why: an escalation makes `report.status != Resolved`, which routes to `FinishResult::Skipped` (`src/resolve/mod.rs:433-439`), so no `StoppedOnConflict` ever arises and the loop cannot continue. Good reasoning.

Three problems:

**AC3 contradicts AC6 and Test 4 (blocking).** AC3 says `--max-rounds 1` "reproduces today's single-stop behavior **exactly**." AC6 says `loop` is "omitted whenever the run behaves as today, so every pre-CLO-554 scenario emits byte-identical JSON." But §4 item 3's omission rule makes `loop` **present** on `CapReached`, and Test 4 asserts `loop.terminal == "cap_reached"` for exactly that invocation. Three readings, and an implementer will pick one and be wrong.

It compounds in §4 item 7, which rewrites `rebase_stops_on_next_conflict_reports_rerun` to `--max-rounds 1` "so it keeps pinning single-stop behavior." That test asserts `stdout.contains("stopped on the next conflicted commit")` (`tests/resolve_integration.rs:1158-1161`), while §4 item 6 changes the cap-reached headline to also name `--max-rounds`. Test 4 wants both the legacy substring *and* `cap_reached` — achievable, but only if the spec says so.

**AC5's "round i of N" is misleading (M3).** N is the cap (default 10), not the number of rounds the rebase needs, which is unknowable until it ends. "Round 1 of 10" on a 2-conflict rebase is simply wrong. Pin the wording — `round 1 (cap 10)`.

**AC3 covers only the CLI zero (M-blocking, see §3).** Nothing says what `[conflict] max_rounds = 0` in `config.toml` does.

Also unstated: what top-level `status` is on the `declined` terminal. From the code it would be `resolved` with `finish.result == stopped_on_conflict` — defensible (it matches today's single-stop shape) but odd next to `loop.terminal == "declined"`, and AC8 makes it consumer-visible. Say it explicitly.

## 3. Constraints & Assumptions

The constraint set is the strongest part of the spec. Two are load-bearing and correct:

- **The seam choice is validated by the code.** `run_resolve_in_repo`'s Local-mode preconditions (`has_state` non-empty `unmerged`, `src/resolve/mod.rs:170-177`) are exactly what `StoppedOnNextConflict` guarantees — `src/git.rs:329-333` returns it only when `has_conflict_state()` *and* `!unmerged.is_empty()`. Round k+1's entry conditions hold by construction. This is the right seam.
- **The double-default warning is earned.** `src/config.rs:124-136` carries a comment documenting the exact prior bug (derived `Default` silently disabling mergiraf and zeroing temperature). Worth the explicit constraint.

Two gaps:

**The `--max-rounds 0` rejection has no valid home (blocking).** §4 item 2 puts it in `resolve_conflict_config` (`src/resolve/mod.rs:508`), which returns `ConflictConfig`, not `Result` — it cannot report an error. It is also called from *inside* `run_resolve_in_repo`, after repo discovery and the `NoConflictInProgress`/`NoConflicts` checks, so "before any work" fails there too. Use clap `value_parser!(usize).range(1..)` on the flag (rejects at parse time, before `Repo::discover`), and specify the config-side rule separately: reject, or clamp to 1 with a warning. Pick one.

Minor upside note: the destructure at `src/resolve/mod.rs:511-522` is exhaustive (no `..`), so adding `max_rounds` to `Commands::Resolve` forces the config-merge edit at compile time. No risk of drift there.

**README is missing from the file list (M).** `README.md:395-415` is the canonical `[conflict]` table plus a sample TOML block. `max_rounds` / `--max-rounds` is user-facing surface and belongs there. The scope line says 8 files; it's 9. Nothing forces this edit the way the compiler forces the config one.

The no-progress guard is sound: `REBASE_HEAD` is written on every conflict stop and names the distinct original commit being replayed, so an unchanged sha genuinely means no forward motion. Handling "absent while the finish reported `StoppedOnConflict`" is a good defensive touch.

## 4. Decomposition / Phases

Sequencing is correct: 1-4 are genuinely independent, and 5's dependency on all four is real. Item 3's report shapes reuse `FileReport`/`FinishReport` verbatim, matching the `staged`/`finish`/`restored` additive precedent at `src/resolve/report.rs:11-30`. Item 4's gate correctly builds on `prompt_choice_from` (`src/ui.rs:33-51`) rather than re-parsing — and the existing `parse_choice` already treats `""`/`no` as No and EOF as No, so the contract reuse is literal, not aspirational.

**Item 7 is under-specified and blocks Test 3 (blocking).** `mock_ollama_server_multiple` (`tests/resolve_integration.rs:115`) returns only `(url, JoinHandle<()>)`. There is no consumed-request counter, so Test 3's "2 queued, only 1 consumed" is unassertable. Worse, `server.join()` will block on the never-arriving second connection until the timeout — and that timeout is a *single shared 10s budget* for the whole sequence (`start` is set once at line 120, outside the `for body in responses` loop), not per response. So AC2's headline guarantee, "zero provider calls spent on the declined round," is untestable as written.

Item 7 needs to add: extend the mock with an `Arc<AtomicUsize>` request count and a way to stop waiting once the driver exits. The same shared-10s budget likely bites Test 11's PTY gate case, which adds human-paced `expect` interaction on top of two signed rounds.

**Item 6 should pin the abort wording (M1).** `print_human_report`'s `Aborted` arm prints *"Aborted - working tree restored, nothing changed."* (`src/resolve/mod.rs:1063`). In a round-2 abort (Test 2), round 1 **is** committed — "nothing changed" is false, and a user who believes it may reach for `git rebase --abort` thinking there's nothing to lose. Item 6 lists "mid-loop abort" as in scope and AC8 covers it implicitly, but no criterion pins the correction.

Related (M2): §3's "keep top-level `files`/`staged`/`finish` = last round" makes the envelope go quiet on exactly this path. The Skip arm returns early at `src/resolve/mod.rs:347-373` with `finish: None`, so a mid-loop abort emits `status: aborted`, `restored: true`, and **no `finish`** — while a rebase sits stopped mid-sequence with round 1 committed. Either carry the last non-`None` `finish`, or state that machine consumers must read `loop.rounds[].finish` on non-completed terminals.

## 5. Risks & Open Questions

The repo's spec template has no risks section (CLO-555 and CLO-564 both run Problem/AC/Constraints/Decomposition/Evaluation), and "Escalate when" partly covers the role — so this isn't a template violation. Substantively, though:

- **Interrupt exposure widens (L3).** No signal handling exists anywhere in the codebase. Ctrl-C during round k's propose leaves the zdiff3 re-checkout unrestored — not new, but the loop multiplies the exposure window by N and puts committed rounds behind it. One sentence stating that mid-loop interruption inherits single-round behavior, with `gcm resolve` as the recovery path, closes it.
- **Test matrix gaps (L2).** No row covers `[conflict].max_rounds` from config with a CLI override, so AC3's precedence half is untested. No row covers config `max_rounds = 0`. The cherry-pick sequence — the whole justification for owner decision 3 — appears only under "edge cases to verify" with no named test or run command, unlike rows 1-12. Given that keying on the signal rather than the op name is a stated design decision, it deserves a numbered row.
- **Provider-call-per-round assumption.** Tests 1 and 3 queue exactly 2 responses for 2 rounds, which holds only if each conflicted commit has exactly one conflicted file with at least one non-trivial hunk. True for the fixture as described, but state it — otherwise a fixture tweak silently changes what the counter proves.
- **`git rebase --abort` discards committed rounds.** Correctly identified and correctly required in the output. This is the sharpest user-facing risk in the feature and the spec caught it.

## Verdict

**NEEDS_REVISION** — the architecture is sound and the seam choice is validated by the code; the blockers are three contained gaps, not a redesign. Estimated fix: one editing pass over §2, §4 items 2 and 7.

## Priority Actions

1. **Resolve the `--max-rounds 1` contradiction** (AC3 / AC6 / §4 item 3 / Test 4 / §4 item 7). State that `--max-rounds 1` preserves git state, exit code, and the legacy headline substring, but additionally emits `loop` with `cap_reached`; scope AC6's byte-identical claim to runs where the user did not set `--max-rounds`; and give Test 4 both assertions explicitly.
2. **Add the mock-harness sub-task to §4 item 7.** `mock_ollama_server_multiple` needs a request counter and a bounded, non-hanging join before AC2 and Test 3 are testable at all. Check the shared 10s accept budget against Test 11's PTY case while you're there.
3. **Move the `--max-rounds 0` rejection out of `resolve_conflict_config`** (it returns a non-`Result` and runs too late) to a clap `value_parser` range, and specify the `[conflict] max_rounds = 0` behavior — reject or clamp.
4. **Pin the mid-loop abort headline in AC4 or AC8.** "Aborted - working tree restored, nothing changed" is false once round 1 is committed, and it misleads toward a destructive `git rebase --abort`.
5. **Add README.md to the file list** (`[conflict]` table at `README.md:395-415`); update the scope line to 9 files.
6. **Specify the envelope on non-completed terminals**: top-level `finish` on mid-loop abort, and top-level `status` on `declined`.
7. **Fix the AC5 banner wording** — `round i (cap N)`, not "round i of N".
8. **Add test rows** for config-vs-CLI `max_rounds` precedence and for the cherry-pick sequence; trim §1's savings claim to process re-invocation and re-prompting.
