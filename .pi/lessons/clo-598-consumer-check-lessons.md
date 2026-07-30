# CLO-598 lessons - out-of-tree consumer check + public surface lock

## L1 - Mechanical guards must be proven to fail before they can be trusted

**Source incident**: CLO-598 implement phase pre-PR validation (Codex FAIL). `scripts/check-public-surface.sh` checked eight fixed HTML paths directly under `target/surface_check/doc/gcm/`, but rustdoc places types in per-module subdirectories (e.g. `doc/gcm/privacy/enum.ScanError.html`). The script always printed "OK" because no `struct.*`/`trait.*` file could ever appear at the checked directory.

**Rule**: Any guard that asserts non-existence of artifacts must be self-tested by temporarily adding a real, currently-public item to the forbidden list and confirming it exits non-zero. Do not trust a green run of a negative check.

**How to apply**: When adding a script that checks "X must not be publicly exported", include a self-test mode or a one-off manual step that flips the expectation and verifies the script fails. Recursive search and all item-kind prefixes (`struct`, `enum`, `trait`, `type`, `fn`) are safer than fixed root-level paths.

## L2 - Out-of-tree consumer tests must exercise real code paths, not just type names

**Source incident**: CLO-598 validation found two smoke tests passed without reaching the code they named. `model_resolution_uses_injected_env` injected `GCM_PROVIDER`, but `resolve_model_with_source` reads per-provider env vars (`GCM_GROQ_MODEL`), so the env path was untested. `model_fetch_degrades_to_fallback` passed `key: None`, short-circuiting before the fetcher ran.

**Rule**: A test that "uses" an API is not enough; it must execute the branch or path it claims to verify. Construct assertions against the return value that would fail if the wrong branch were taken.

**How to apply**: For public-surface tests, pair every imported type with a construction or call, and assert on the resulting value / variant. When injecting env lookups, match the exact env-var names the implementation reads; when testing fallback, force the failure path with valid-but-failing inputs rather than no-key short circuits.

## L3 - Cargo `publish = false` is represented as `[]`, not `null`

**Source incident**: CLO-598 plan ST4 acceptance text claimed `cargo metadata` returns `publish: null` for `publish = false`. `cargo metadata` actually returns `publish: []`; `null` means unrestricted publishing (the opposite).

**Rule**: When asserting on Cargo metadata fields, verify the actual JSON representation rather than inferring from the TOML syntax.

**How to apply**: Use `cargo metadata --no-deps --format-version 1 | jq '.packages[0].publish'` and check for `[]` when `publish = false` is intended. Treat `null` as a red flag for missing or misconfigured `publish`.
