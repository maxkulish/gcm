# AI Review: CLO-597 — Expose source-attributed status resolution through the gcm library

**Reviewer**: Gemini 2.5 Flash
**Date**: 2026-07-30
**Design doc**: `docs/designs/clo-597-expose-status-resolution.md`

## Verdict: APPROVE_WITH_SUGGESTIONS

The proposed design is extremely solid, elegant, and strictly adheres to both ADR-002 and the successful patterns established in CLO-595/596.

## Key Findings

- **Flawless Facade & Type Identity Alignment:** Using `#[path = ".../facade.rs"] mod ...` combined with `pub use gcm::<module>::*` ensures the binary uses the actual library target's types, avoiding compilation duplication or type identity mismatches.
- **Separation of Binary Constants:** Moving `SCHEMA_VERSION` and `VERSION` out of the library's `build_report` signature and passing them as arguments from the facade keeps the library completely clean of binary-side constants.
- **Preservation of Byte-Identity:** Since the CLI facade passes the exact same parameters and the output-formatting structs are unmodified, `gcm status` outputs and exit codes remain byte-identical to v0.6.0.

## Actionable Feedback

1. **`tests/status.rs` should remain unchanged.** The design plan mentions modifying `tests/status.rs` to update imports from `crate::status` to `gcm::status`. However, `tests/status.rs` operates by running the compiled CLI binary (`CARGO_BIN_EXE_gcm`) as a subprocess and does not import any symbols from `crate::status` or `gcm::status` directly. Thus, `tests/status.rs` should remain completely unchanged.

2. **Relocate unit tests for extracted Ollama helpers.** When moving `normalize_host`, `is_cloud_model`, and `has_port` from `src/provider/ollama.rs` to `src/provider/identity.rs`, also relocate their corresponding unit tests (`normalize_host_variants` and `is_cloud_model_detects_both_suffixes`) to keep implementation and verification co-located.

3. **Determine placement of pure wizard helpers.** Helper functions like `wizard_model_list`, `wizard_model_hint`, and `wizard_persist_key` are pure and do not use `cliclack` or `GcmError`. Since they are pure, they can live in the library `src/config/mod.rs`. Recommendation: place them in the library to keep the wizard facade purely focused on interactive TTY operations.

4. **Sanity check on feature example.** Ensure that `tests/library_status_api.rs` is compiled and executed when running with `--no-default-features` (or is properly ignored if it relies on any default-enabled library components).