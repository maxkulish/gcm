# Plan: CLO-555 - Rework `gcm resolve` into an ownership transaction

**Source of truth**: `docs/specs/2026-07-12-clo-555-resolve-ownership-transaction.md` (Section 4 decomposition; all technical detail lives there, not here). Design doc: `docs/hotfix/2026-07-12-resolve-stage-and-finish.md`.
**Branch**: `feat/clo-555-resolve-transaction`
**Overall Progress**: 100% (10/10 tasks completed)

## Phase 1: Independent foundations

- [x] ST1 (42f66a5) - shared prompt parser (`src/ui.rs`): pure `parse_choice`, BufRead loop, 3-attempt reprompt, EOF/empty = No, `[y/N/e(dit)]`, both prompts routed; unit tests first
- [x] ST2 (599d9e7) - byte snapshot/restore (`src/git.rs`, `src/resolve/mod.rs`): `read_file_bytes`/`write_file_bytes`, snapshot before zdiff3, restore helper with external-modification guard; byte-exactness tests (CRLF, no trailing newline)
- [x] ST3 (dbc1794) - finish helper (`src/git.rs`, `src/error.rs`): `FinishOutcome`, `finish_conflict_op()` (dispatch rebase -> cherry-pick -> merge, `git commit -S --no-edit` / `-c commit.gpgsign=true <op> --continue`, GIT_EDITOR=true, stdin inherited, postcondition classification), `GcmError::FinishFailed` with `leaves_staged() == true`; probe-gated signing tests

## Phase 2: Transaction core (landed as one commit - the three stages replace a single flow)

- [x] ST4a (831c15c) - proposal building: `resolve_file` -> pure `propose_file`, mergiraf + marker-free as proposals, validation-retry stays here, dead dry-run arm removed
- [x] ST4b (831c15c) - confirm + abort/restore: confirm-all loop, edit via $EDITOR + validation, rejection -> guarded restore + Aborted report + exit 0
- [x] ST4c (831c15c) - apply + central staging: write confirmed proposals, stage exact paths by final disposition, escalation stages confirmed work only

## Phase 3: Finish + surfaces

- [x] ST5 (5ecba32) - finish integration + `--no-finish` + `ResolveMode` (`src/resolve/mod.rs`, `src/cli.rs`); existing full-resolve tests gained `--no-finish` for CI determinism
- [x] ST6 (cbb67a2) - remote gating: commit/push only on Resolved/Noop; Aborted keeps nothing (no commit, no push, scratch dropped)
- [x] ST7 (f214cfb) - output: headlines, `Aborted`/`Rejected` variants, `staged`/`finish{result,commit,op}`/`restored` fields, `--json` preview to stderr

## Phase 4: Docs + end-to-end matrix

- [x] ST8 (6ed99ab + docs commit) - README rewrite (transaction contract, breaking-change + SIGINT callouts, `--no-finish`), `signing_available()` probe helper, cargo matrix in `tests/resolve_integration.rs` (signed finish, marker-free, escalation, hook failure, cherry-pick, rebase-stops, fake-mergiraf) + `tests/resolve_remote.rs` (Partial never commits/pushes, Resolved single commit), acceptance AC-R1/AC-R2 expect cases (interactive rejection/EOF restore)

## As-built deviations (detail in spec "Implementation notes")

- D1: `FinishResult::Failed` dropped - finish failure is the `FinishFailed` error envelope, exit non-zero
- D2: `NothingToFinish` defensive-only (local entry gate precedes); unit-tested
- D3: interactive eval rows in `scripts/acceptance.sh` (expect), not cargo - non-TTY guard kept
- Bonus fixes: `ConflictConfig` Default mismatch (44ce964); marker-free files excluded from zdiff3 re-checkout (4a7e29e)
