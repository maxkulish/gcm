# Design Review Synthesis: CLO-598

**Verdict:** APPROVE_WITH_SUGGESTIONS

## Review Summary

Gemini identified 5 actionable suggestions, all additive improvements that strengthen the design without contradicting the chosen approach. No suggestions were flagged.

## Applied Suggestions

1. **[HIGH] File-existence check instead of grep** — The surface check script will use rustdoc HTML file existence (`trait.Provider.html`, `struct.ConflictHunk.html`, etc.) instead of grep, avoiding false positives from substring matching (e.g., `Provider` matching `ProviderId`).

2. **[HIGH] Isolated CARGO_TARGET_DIR** — The surface check script will use `CARGO_TARGET_DIR=target/surface_check` to avoid stale doc artifacts causing false positives.

3. **[HIGH] Smoke feature propagation** — The smoke crate will define a `gcm-cli` feature that propagates `gcm/cli`, enabling both `cargo test` (library-only) and `cargo test --features gcm-cli` (with CLI deps) in CI.

4. **[MEDIUM] `publish = false` in Cargo.toml** — Both the root `Cargo.toml` and `smoke/Cargo.toml` will have `publish = false` to mechanically enforce ADR-002 Decision 7.

5. **[MEDIUM] Script portability** — The surface check script will use `git rev-parse --show-toplevel` for path resolution.

## Flagged Suggestions

None. All suggestions are additive and consistent with the chosen approach.

## Design Doc Updates

The design doc at `docs/designs/clo-598-consumer-check.md` has been updated to reflect all 5 applied suggestions.