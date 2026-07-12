# Plan: CLO-555 - Rework `gcm resolve` into an ownership transaction

**Source of truth**: `docs/specs/2026-07-12-clo-555-resolve-ownership-transaction.md` (Section 4 decomposition; all technical detail lives there, not here). Design doc: `docs/hotfix/2026-07-12-resolve-stage-and-finish.md`.
**Branch**: `feat/clo-555-resolve-transaction`
**Overall Progress**: 10% (1/10 tasks completed)

## Phase 1: Independent foundations

- [x] ST1 (42f66a5) - shared prompt parser (`src/ui.rs`): pure `parse_choice`, BufRead loop, 3-attempt reprompt, EOF/empty = No, `[y/N/e(dit)]`, both prompts routed; unit tests first
- [ ] ST2 - byte snapshot/restore (`src/git.rs`, `src/resolve/mod.rs`): `read_file_bytes`/`write_file_bytes`, snapshot before zdiff3, restore helper with external-modification guard; byte-exactness tests (CRLF, no trailing newline)
- [ ] ST3 - finish helper (`src/git.rs`, `src/error.rs`): `FinishOutcome`, `finish_conflict_op()` (dispatch rebase -> cherry-pick -> merge, `git commit -S --no-edit` / `-c commit.gpgsign=true <op> --continue`, GIT_EDITOR=true, stdin inherited, postcondition classification), `GcmError::FinishFailed` with `leaves_staged() == true`; probe-gated signing tests

## Phase 2: Transaction core

- [ ] ST4a - proposal building (`src/resolve/mod.rs`): `resolve_file` -> pure proposal builder (no write/prompt), mergiraf + marker-free as proposals, validation-retry stays here, dead dry-run arm removed
- [ ] ST4b - confirm + abort/restore (`src/resolve/mod.rs`): confirm-all loop, edit via $EDITOR + validation, rejection -> guarded restore + Aborted report + exit 0
- [ ] ST4c - apply + central staging (`src/resolve/mod.rs`): write confirmed proposals, stage exact paths by final disposition, escalation stages confirmed work only

## Phase 3: Finish + surfaces

- [ ] ST5 - finish integration + `--no-finish` + `ResolveMode` (`src/resolve/mod.rs`, `src/cli.rs`, `src/main.rs`)
- [ ] ST6 - remote gating (`src/resolve/remote/mod.rs`): commit/push only on Resolved/Noop; Aborted keeps nothing
- [ ] ST7 - output (`src/resolve/report.rs`, `src/resolve/mod.rs`, `src/ui.rs`): headlines, `Aborted`/`Rejected` variants, `staged`/`finish`/`restored` fields, `--json` preview to stderr

## Phase 4: Docs + end-to-end matrix

- [ ] ST8 - README rewrite (AC11 incl. breaking-change + SIGINT callouts), `signing_available()` probe helper, integration matrix in `tests/resolve_integration.rs` + `tests/resolve_remote.rs` (spec Section 5 rows 3-15)
