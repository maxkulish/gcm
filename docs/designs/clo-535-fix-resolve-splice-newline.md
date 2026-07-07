# Design: CLO-535 - Fix `gcm resolve` splice: resolution missing a trailing newline joins the following line

## Problem

Users running `gcm resolve` on a conflict whose LLM-provided resolution text omits a final newline get output where the first context line after the hunk is fused onto the last resolved line (for example, a closing `}` pulled up onto a `format!` call). The merge decision is correct, but the file is mis-formatted. Discovery (`docs/discovery/clo-535.md`) traced this to `reconstruct(file, resolutions, original)` in `src/resolve/mod.rs`, which appends provider resolution text verbatim and trusts it to end with `\n` before the non-hunk branch appends the following context line with its own `\n`. The bug is provider-independent (observed with both Groq and Gemini) and surfaced during the CLO-531 post-merge smoke test, so it affects any conflict where the model forgets the trailing newline - which no prompt or schema can reliably prevent. The escalated branch is unaffected because it emits `\n` explicitly after each re-emitted original line.

## Goals / Non-goals

**Goals**

- Guarantee that a resolution lacking a trailing newline does not fuse with the following context line (PRD O1, S1).
- Use the file's dominant line ending for the guard: `\r\n` for CRLF files, otherwise `\n` (PRD O2, S2, FR-1, FR-2).
- Avoid introducing extra blank lines when the resolution already ends with a newline (PRD S3).
- Preserve the existing final-newline trim for files that originally had no trailing newline (PRD O3, S4, FR-3).
- Add a unit test that fails before the fix and passes after (PRD O4, S5, FR-4).

**Non-goals**

- Changing how providers are prompted or constraining the response schema to require a trailing newline (discovery Approach B, explicitly rejected).
- Trimming or rewriting trailing whitespace inside resolution text (discovery Approach C, not chosen).
- Any change to auto-staging, `--continue`, batching, validation-retry, CLI flags, or configuration.

## Architecture

The change is confined to the resolution branch of one private function; no new modules or types are introduced.

- **File:** `src/resolve/mod.rs`
- **Function:** `reconstruct(file: &ConflictFile, resolutions: &[Option<String>], original: &str) -> String`

Data flow (unchanged except for the marked step):

```text
resolve_file()
  └─ reconstruct(&file, &resolutions, &content)
       for each source line 1..=N:
         ├─ at hunk start with Some(text):   push resolution text
         │                                   [NEW] ensure it ends in the file's line ending
         ├─ at hunk start with None:         re-emit original hunk lines + '\n' (already correct)
         └─ otherwise:                       push context line + '\n'
       └─ if !original.ends_with('\n'): out.pop()   (unchanged)
```

The new step sits immediately after the resolution text is pushed (the `if let Some(text)` block at `src/resolve/mod.rs:608-616`), inside the same branch, before `line_no` advances to the next context line. It reuses the already-computed `uses_crlf: bool` (`src/resolve/mod.rs:602`) so no new line-ending detection is added. Because the guard runs per hunk and only appends when the ending is absent, the terminal `out.pop()` (`src/resolve/mod.rs:635`) continues to govern the file's final newline correctly: at end-of-file the last hunk's guarded newline is exactly the one `pop()` removes when the original had no trailing newline.

CRLF handling composes with the existing normalization: the LF→CRLF `text.replace('\n', "\r\n")` path already runs before the guard, so checking for a trailing `\r\n` on the pushed bytes is sufficient. Checking `out.ends_with(...)` (rather than `text.ends_with(...)`) is preferred because `out` reflects the CRLF-normalized bytes actually appended.

## Public API surface

`reconstruct` is a private `fn` in `src/resolve/mod.rs`; its signature is unchanged and no public trait, struct, or exported item is touched.

```rust
// Signature - UNCHANGED
fn reconstruct(file: &ConflictFile, resolutions: &[Option<String>], original: &str) -> String
```

Before (resolution branch, `src/resolve/mod.rs:608-616`):

```rust
if let Some(text) = &resolutions[hunk_idx] {
    // Normalize resolution text line endings to match the file.
    if uses_crlf && !text.contains("\r\n") {
        // Convert LF to CRLF in the resolution text.
        let normalized = text.replace('\n', "\r\n");
        out.push_str(&normalized);
    } else {
        out.push_str(text);
    }
}
```

After (add a trailing-line-ending guard within the same branch):

```rust
if let Some(text) = &resolutions[hunk_idx] {
    // Normalize resolution text line endings to match the file.
    if uses_crlf && !text.contains("\r\n") {
        // Convert LF to CRLF in the resolution text.
        let normalized = text.replace('\n', "\r\n");
        out.push_str(&normalized);
    } else {
        out.push_str(text);
    }
    // Guard: a resolution without a trailing newline must not fuse with the
    // following context line. Append exactly one line ending if missing.
    if uses_crlf {
        if !out.ends_with("\r\n") {
            out.push_str("\r\n");
        }
    } else if !out.ends_with('\n') {
        out.push('\n');
    }
}
```

## Assumptions

- The resolution text stored in `resolutions[i]` is the raw provider replacement and is the only splice path missing the newline guard (the escalated branch already emits `\n`). Confidence: high. Verify: `src/resolve/mod.rs:617-625` and discovery report.
- For CRLF files, checking `out.ends_with("\r\n")` after the existing normalization is correct even when the resolution mixes endings, because the normalization runs only when `!text.contains("\r\n")`; a resolution already containing `\r\n` is pushed verbatim and its own ending is honored. Confidence: medium. Verify: unit test `reconstruct_crlf_resolution_missing_newline` plus a mixed-ending case.
- The terminal `out.pop()` removes at most one `\n`, so guarding at end-of-file cannot leave a CRLF file with a dangling `\r`. Confidence: medium. Verify: unit test asserting a CRLF file whose original lacked a final newline is byte-identical in that respect after reconstruct. If `pop()` proves insufficient for the CRLF-no-final-newline corner, this becomes an open question (see below).
- No caller depends on the current (buggy) fused output. Confidence: high. Verify: `cargo test` across the suite, including `tests/resolve_integration.rs`.

## Test plan

**Unit tests** (in `src/resolve/mod.rs` `#[cfg(test)] mod tests`, alongside `reconstruct_replaces_hunk_with_resolution`):

- `reconstruct_resolution_missing_newline_keeps_following_line` - LF file, resolution `"resolved"` (no `\n`); assert the output contains `"resolved\nline 2"` and never `"resolvedline 2"`. This is the regression test that fails before the fix.
- `reconstruct_resolution_with_newline_no_double_blank` - LF file, resolution `"resolved\n"`; assert no `"resolved\n\n"` (guard must not add a second newline).
- `reconstruct_crlf_resolution_missing_newline` - CRLF file (`original.contains("\r\n")`), resolution `"resolved"`; assert the splice uses `"resolved\r\n"` and the following context line stays separate.
- `reconstruct_no_final_newline_preserved` - LF file whose `original` does not end in `\n` and whose last hunk resolution lacks a trailing newline; assert `!out.ends_with('\n')` (existing trim behavior preserved, FR-3).

Line-ending × trailing-newline matrix (each row = one unit assertion):

| Ending | Resolution ends in newline? | Original ends in newline? | Expected splice / file tail |
|---|---|---|---|
| LF | no | yes | one `\n` inserted; file ends `\n` |
| LF | yes | yes | unchanged; no double `\n` |
| LF | no | no | one `\n` inserted mid-file; file tail trimmed to no `\n` |
| CRLF | no | yes | one `\r\n` inserted; file ends `\r\n` |
| CRLF | yes | yes | unchanged; no double `\r\n` |

**Integration tests** (`tests/resolve_integration.rs`): no new scenario required for this bug, but run the existing suite to confirm the end-to-end resolve path is unaffected. This task is `has_backend: false` (no provider trait change), so no per-backend/provider matrix applies; the fix is provider-independent string handling exercised entirely at the `reconstruct` unit level.

**Manual verification**: reproduce with a two-line file whose conflict resolution omits the final newline, run `gcm resolve --dry-run`, and confirm the previewed output keeps the trailing context line on its own line. Then run the pre-merge gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.

## Migration / rollout

The change is a pure bug fix, backward compatible, and additive in behavior only: correctly-formatted resolutions (already ending in a newline) produce byte-identical output to today, and mis-formatted ones gain a single line ending. No feature flag, config key, schema version bump, or CLI flag is introduced. No migration steps for users; it ships in the normal release of `gcm`. Rollout order is a single PR against `main` gated on `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.

## Open questions

- CRLF end-of-file corner: when a CRLF file's original lacks a final newline and the last hunk resolution also lacks one, the guard appends `\r\n` and the terminal `out.pop()` removes only the `\n`, potentially leaving a dangling `\r`. The chosen minimal-diff approach (Approach A) does not touch `pop()`. Tradeoff: extending the trim to strip a trailing `\r\n` pair is more correct for this rare case but widens the diff and risks altering the existing well-tested no-final-newline behavior for LF files. Left open pending the `reconstruct_crlf_resolution_missing_newline` result - if the dangling-`\r` case is confirmed, decide between a targeted `\r` trim and accepting the corner as out of scope.
