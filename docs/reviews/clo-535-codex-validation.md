# Pre-PR validation: clo-535

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- HIGH: CRLF files without a final newline still leave a dangling ``. The guard appends `
` for CRLF output at [src/resolve/mod.rs](/Users/mk/Code/gcm--fix-clo-535-new-line/src/resolve/mod.rs:619), but the final trim still removes only one char at [src/resolve/mod.rs](/Users/mk/Code/gcm--fix-clo-535-new-line/src/resolve/mod.rs:644). For a last-hunk replacement like `"resolved"`, the result is `resolved`. This violates the plan's CRLF/no-final-newline requirement in [docs/plans/clo-535-fix-resolve-splice-newline.md](/Users/mk/Code/gcm--fix-clo-535-new-line/docs/plans/clo-535-fix-resolve-splice-newline.md:28). The new test at [src/resolve/mod.rs](/Users/mk/Code/gcm--fix-clo-535-new-line/src/resolve/mod.rs:773) misses this because it checks `!ends_with("\n")` and `!ends_with('\n')`, but not `!ends_with('')`.

- MEDIUM: Empty replacements now create an extra blank line. Existing code can produce a valid empty auto-resolution for both-empty hunks at [src/resolve/classify.rs](/Users/mk/Code/gcm--fix-clo-535-new-line/src/resolve/classify.rs:150), and provider responses also allow an empty replacement string via [src/provider/mod.rs](/Users/mk/Code/gcm--fix-clo-535-new-line/src/provider/mod.rs:227). `resolve_file` stores those as `Some(text)` at [src/resolve/mod.rs](/Users/mk/Code/gcm--fix-clo-535-new-line/src/resolve/mod.rs:315), then the new guard appends a newline even when `text == ""`. That turns "delete the hunk" into "insert a blank line."

- LOW: `git diff --check main...HEAD` fails due trailing whitespace in [docs/discovery/clo-535.md](/Users/mk/Code/gcm--fix-clo-535-new-line/docs/discovery/clo-535.md:3).

## Missing Items

- CRLF no-final-newline behavior is not actually preserved.
- The CRLF no-final-newline test does not assert the key failure mode: a dangling ``.
- Empty replacement behavior is untested and regressed.
- I could not independently run `cargo clippy` or `cargo test` in this read-only session. `cargo fmt --check` passed, but Cargo test/clippy failed before compiling because it could not open `target/debug/.cargo-lock`.

## Recommendations

- Fix the final trim to remove the full line ending for CRLF files, for example by truncating `\r\n` as a pair when `uses_crlf && !original.ends_with('\n')`.
- Add `assert_eq!(out, "resolved")` or at least `assert!(!out.ends_with('\r'))` to the CRLF no-final-newline test.
- Skip the guard for `text.is_empty()` and add a regression test showing an empty replacement between two context lines does not insert a blank line.
- Remove the trailing spaces in `docs/discovery/clo-535.md`.
- Re-run the full pre-merge gate in a writable environment.
