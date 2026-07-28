# Pre-PR validation: clo-595

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-28
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

- LOW: [src/privacy/rules.rs](/Users/mk/Code/gcm--feat-clo-595-secret-scanner/src/privacy/rules.rs:38) still has redundant `#[allow(dead_code)]` attributes on public API items, despite the design explicitly saying they should be removed. Also applies at lines 41, 94, and 99.

- LOW: [src/privacy/mod.rs](/Users/mk/Code/gcm--feat-clo-595-secret-scanner/src/privacy/mod.rs:217) only compares range counts in `scanner_ranges_match_detect_secret_ranges`, not the actual ranges the design calls out. [src/error.rs](/Users/mk/Code/gcm--feat-clo-595-secret-scanner/src/error.rs:249) also tests `SecretDetected` and `InvalidMode`, but not the `RulePack -> GcmError::Config` mapping specified in the design.

- LOW: `git diff --check main...HEAD` fails on trailing whitespace in [docs/prds/clo-595-ship-gcms-secret-scanner-as-a-library-api-consumable-by-other-crates.md](/Users/mk/Code/gcm--feat-clo-595-secret-scanner/docs/prds/clo-595-ship-gcms-secret-scanner-as-a-library-api-consumable-by-other-crates.md:48).

## Missing Items

Core implementation matches the design: `[lib]` exists, `privacy` is the only exported module, `SecretScanMode::resolve_with` removes direct process-env reads from the shared path, `ScanError` avoids `GcmError` in library consumers, and `tests/library_api.rs` covers the external-consumer surface.

Two non-core items remain: the README library section from the rollout step is absent, and I did not independently rerun the v0.5.2 CLI parity/manual checks in this read-only sandbox. The untracked validation synthesis records `cargo test`, `cargo build --no-default-features`, and dependency-tree checks as green.

## Recommendations

Commit or drop the current dirty working-tree changes before PR; notably, the working tree already fixes the `Privacy::scan_text` mapping to use `map_err(GcmError::from)`, while `HEAD` still has the manual match.

Do not hand-bump `Cargo.toml` to `0.6.0` on this feature branch unless the release workflow changed; `Makefile` owns that via `make release-minor`.

Clean the three low-risk items: remove the stale `allow(dead_code)` attributes, strengthen the two tests, and strip the PRD whitespace. Then run the manual parity checks from the design before merge.
