# Plan: CLO-535 Fix `gcm resolve` splice: resolution missing a trailing newline joins the following line

## Context

- Design: `docs/designs/clo-535-fix-resolve-splice-newline.md`
- Discovery: `docs/discovery/clo-535.md`
- PRD: `docs/prds/clo-535-fix-resolve-splice-newline.md`
- Linear: https://linear.app/cloud-ai/issue/clo-535/fix-gcm-resolve-splice-resolution-missing-a-trailing-newline-joins-the
- Branch: `fix/clo-535-new-line`

## Sub-tasks

### ST1 Implement trailing-newline guard in `reconstruct`
**Files:** `src/resolve/mod.rs`
**Acceptance:** `cargo test reconstruct_` passes / pre-merge gate green
**Estimate:** S

In the resolution branch of `reconstruct`, after pushing `text` (or the CRLF-normalized variant), append exactly one line ending if missing. Use `\r\n` when `uses_crlf` is true, otherwise `\n`. Do not touch the escalated branch or the final `out.pop()` trim.

### ST2 Add unit tests for missing-trailing-newline and line-ending matrix
**Files:** `src/resolve/mod.rs` (test module)
**Acceptance:** `cargo test reconstruct_resolution_missing_newline_keeps_following_line` passes and a temporary pre-fix baseline confirms it fails without the guard
**Estimate:** S

Add tests covering:
- LF resolution without trailing newline keeps the following context line separate.
- LF resolution with trailing newline does not produce a double blank.
- CRLF resolution without trailing newline uses `\r\n` at the splice.
- CRLF file with no final newline and last-hunk resolution without trailing newline does not leave a dangling `\r`.
- File with no final newline preserves its original tail behavior.

### ST3 Run pre-merge gate and fix any regressions
**Files:** repo-wide
**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` is green
**Estimate:** S

Run the full pre-merge gate. Address any new warnings or test failures introduced by ST1/ST2, including the existing integration suite in `tests/resolve_integration.rs`.

## Pre-merge gate

- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## Risks

- The CRLF no-final-newline corner case may require extending the terminal trim from a single `\n` to a trailing `\r\n` pair, slightly widening the diff.
- If the provider already sends mixed line endings, the normalization logic (`text.contains("\r\n")`) may behave unexpectedly; the test matrix covers this.
