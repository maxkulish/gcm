# Pre-PR validation: clo-598

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-30
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- HIGH: The new checks are not wired into CI. The design says CI should run `cd smoke && cargo test`, `cd smoke && cargo test --features gcm-cli`, and `scripts/check-public-surface.sh`. Current CI still only runs root fmt/clippy/test/build. Since `smoke/` is intentionally standalone, root `cargo test --all` will not run it.

- HIGH: The smoke crate does not actually exercise the specified public surface. The design lists `detect`, `ScanError`, `ModelSource`, `ModelFetchOutcome`, status types, and `gcm::paths::xdg_gcm_dir_from` as part of the surface to verify, but the smoke test imports only a subset. The privacy test constructs scanners but never scans or redacts text.

- MEDIUM: Two smoke tests hit the wrong runtime branches. `model_resolution_uses_injected_env` injects `GCM_PROVIDER`, but `resolve_model_with_source` reads per-provider model env vars such as `GCM_GROQ_MODEL`. `model_fetch_degrades_to_fallback` passes `None` for Groq's key, so the function short-circuits before the failing fetcher is called.

- MEDIUM: `scripts/check-public-surface.sh` is not a complete mechanical public API guard. It checks only fixed files directly under `target/surface_check/doc/gcm`, so nested public leaks like `gcm::resolve::ResolveReport` would live under a module subdirectory. Rustdoc also omits `#[doc(hidden)]` public items, which are still externally usable.

- LOW: The plan's publish acceptance is incorrect. `publish = false` is implemented correctly, but `cargo metadata` reports it as `publish: []`, not `null`; `null` is the unrestricted/default publish state.

## Missing Items

- CI steps for both smoke feature combinations.
- CI step for `scripts/check-public-surface.sh`.
- CI or pre-merge step for `cargo test --no-default-features --lib`.
- Smoke assertions for actual privacy detection/redaction, model env source attribution, injected fetcher fallback, status result typing, and `xdg_gcm_dir_from`.
- A public-surface guard that catches nested and doc-hidden public leaks.

## Recommendations

- Update `.github/workflows/ci.yml` to run: `cargo test --manifest-path smoke/Cargo.toml --locked`, `cargo test --manifest-path smoke/Cargo.toml --locked --features gcm-cli`, `cargo test --no-default-features --lib --locked`, and `scripts/check-public-surface.sh`.

- Strengthen `smoke/src/main.rs` by mirroring the existing in-package library tests from the standalone crate: call `detect::secret_ranges`, `Scanner::scan`, assert `ScanError`, use `GCM_GROQ_MODEL` and assert `ModelSource::Env`, pass `Some("sk-test")` to force the fetcher path, and add a direct `xdg_gcm_dir_from` test.

- Replace or augment the rustdoc file check with negative compile checks from a scratch external crate importing forbidden paths. That catches real public usability, including nested and doc-hidden exports.
