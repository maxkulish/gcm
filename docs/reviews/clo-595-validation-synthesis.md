# Pre-PR validation: clo-595

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-28
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | PASS_WITH_NOTES, 3 LOW findings. Read-only sandbox: could not rerun the build/test matrix or the v0.5.2 parity checks; I ran the matrix myself. |
| Gemini | OK | PASS_WITH_NOTES, 3 LOW findings. Full overlap with Codex on `allow(dead_code)` and PRD whitespace. |
| Claude fallback | SKIPPED | Both external reviewers succeeded. |

Verification I ran against the working tree: `cargo fmt --check` clean, `cargo clippy --all-targets -- -D warnings` and `cargo clippy --no-default-features -- -D warnings` both clean, `cargo test` 489 passed / 0 failed (baseline 475) including the `src/lib.rs` doctest, `cargo test --no-default-features --lib` 39 passed, `--test library_api` 4 passed, `cargo build --no-default-features` clean, `cargo tree --no-default-features -e normal` has zero `clap`/`cliclack`/`console`/`ureq` (PRD AC5). `git diff --check main...HEAD` fails on PRD line 48.

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- **The branch is dirty and `HEAD` is not the state that was validated.** `src/privacy/facade.rs` carries round 1's fix (`.map_err(GcmError::from)`) unstaged, `docs/status/clo-595-workflow.yaml` is modified, and the three `docs/reviews/clo-595-*` files are untracked. Opening the PR from `HEAD` ships the hand-rolled `match` that round 1 rejected and discards the fix; every green result above describes the working tree, not `HEAD`.
- **`src/privacy/rules.rs:38,41,94,99` still carry `#[allow(dead_code)]`.** The design's Public API surface section explicitly says these come off once the items are library-public. Zero-risk: `pub` items in a `[lib]` target don't trigger the dead-code lint. Keep `rules.rs:25` (`RawRule::confidence`) — private field.
- **Two design-specified assertions are weaker than specified.** `scanner_ranges_match_detect_secret_ranges` (`src/privacy/mod.rs:214`) compares `.len()` where the design asks for range equality; `scan_error_maps_to_gcm_error` (`src/error.rs:249`) omits the `RulePack -> GcmError::Config` arm, which is reachable in production via `Scanner::vendored` at `src/privacy/facade.rs:17`.
- **Trailing whitespace in the PRD, line 48** — makes `git diff --check main...HEAD` exit non-zero.

## Out of Scope / Deferred
- **No README library section** (design rollout step 5). Real divergence from the rollout narrative, not from an acceptance criterion — PRD S6's docs requirement is met by the green `src/lib.rs` doctest plus `tests/library_api.rs`. Defer to the first `lok` / `remem-ai` integration.
- **Manual v0.5.2 parity run** (design test plan steps 1-7), not executed in either round. Parity is structural here — `Privacy::scan_text` is a pure delegation, so one copy of the off/redact/abort branch exists — and I diffed both user-visible strings against `main` at source level (`git show main:src/privacy/mod.rs:37` versus `ScanError::InvalidMode`'s `Display`, both operating on the trimmed lowercased value). Confirmation step, not an open defect.

## False Positives / Tooling Artifacts
- **"Bump `Cargo.toml` to 0.6.0"** — both reviewers correctly demoted it this round, and it stays out. `Makefile:195` (`_bump-and-tag`) rewrites the version line itself, aborts on a dirty tree, and lands the bump as its own `release: v0.6.0` commit plus tag. Hand-editing now would make the next `make release-minor` compute `0.6.0 -> 0.7.0`. Satisfy the design's 0.5.2 -> 0.6.0 intent after merge with `make release-minor` (not `release-patch`) and note that in the PR description.
- **Codex could not rerun the matrix** — sandbox limitation, resolved by my run above.

## Recommendation
**PROCEED_WITH_FIXES.** The extraction matches the design where it counts: `[lib]` declared, `privacy` the only exported module, the binary-only facade kept out of the library via `#[path]` at `src/main.rs:11`, `resolve_with` taking a caller-supplied closure so the shared path never reads process env, `ScanError` keeping `GcmError` out of every public library signature, and a no-default dependency tree verifiably free of `clap`/`cliclack`/`console`/`ureq`. Nothing found in either round touches the shipped CLI's correctness. One bounded iteration closes the list: commit the working tree (facade fix, status yaml, review docs), delete the four `#[allow(dead_code)]` attributes in `rules.rs`, compare full range vectors in `scanner_ranges_match_detect_secret_ranges`, add the `RulePack` arm to `scan_error_maps_to_gcm_error`, strip the PRD trailing whitespace, then re-run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`. Leave the package version at 0.5.2.

Synthesis written to `docs/reviews/clo-595-validation-synthesis.md` (superseding the round-1 content).
