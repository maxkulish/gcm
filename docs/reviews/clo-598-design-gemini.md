# Gemini Design Review: CLO-598

**Verdict:** APPROVE_WITH_SUGGESTIONS

## Key Findings & Blind Spots

### 1. [HIGH] Naive Grep Over-matching (The "Provider" Substring)
If `scripts/check-public-surface.sh` runs a simple `grep` for forbidden types like `Provider` or `Resolution` against the generated doc directory, it will fail:
- `Provider` matches `ProviderId` and `ProviderStatus` (both of which are public and valid).
- `Resolution` could match `HunkResolution` or `resolve_model_with_source`.
- **Solution:** Instead of string-searching HTML text, check for the *existence* of specific generated rustdoc HTML files (e.g., `trait.Provider.html`, `struct.ConflictHunk.html`, `struct.ResolveContext.html`). Since `cargo doc` only generates files for publicly exported items, the existence of these files is a binary, foolproof indicator of a leak.

### 2. [HIGH] Cargo Feature Caching & Clobbering
If a developer builds docs or runs tests locally, files under `target/doc` persist. If they run `cargo doc` with default features, and then run `scripts/check-public-surface.sh`, the old `ConflictHunk.html` files will still exist in `target/doc/gcm/`, causing a false positive.
- **Solution:** Isolate the doc build target directory in the script by passing `CARGO_TARGET_DIR=target/surface_check`. This guarantees a clean, un-clobbered run without blowing away the developer's main cargo cache.

### 3. [HIGH] Smoke Feature Propagation
The design says: "Verify the consumer crate compiles and tests pass with default features." But if `smoke/Cargo.toml` declares `gcm = { path = "..", default-features = false }`, `smoke` will always compile `gcm` with library-only features.
- **Solution:** Define an explicit feature in `smoke/Cargo.toml` that propagates `gcm`'s CLI features so both states can be validated in CI:
  ```toml
  [features]
  default = []
  gcm-cli = ["gcm/cli"]
  ```

### 4. [MEDIUM] Mechanical Enforcement of ADR-002 Decision 7 (Publish Decision)
To ensure `gcm` is not accidentally published to crates.io before we are ready, we should reinforce the documented publish decision directly in the `Cargo.toml` files.
- **Solution:** Add `publish = false` to both `smoke/Cargo.toml` and `gcm`'s root `Cargo.toml`.

### 5. [MEDIUM] Script Portability
If `scripts/check-public-surface.sh` is run from a subdirectory or CI runner, pathing might break.
- **Solution:** Ensure the script resolves its paths relative to the repository root using `git rev-parse --show-toplevel` or `cd "$(dirname "$0")/.."`.

## Prioritized Actionable Items

1. **[HIGH] Refine `scripts/check-public-surface.sh` to target exact file paths** — use `CARGO_TARGET_DIR=target/surface_check cargo doc --lib --no-default-features` and assert that specific HTML files do NOT exist
2. **[HIGH] Update `smoke/Cargo.toml` to support feature propagation** — add `gcm-cli` feature that propagates `gcm/cli`
3. **[MEDIUM] Prevent accidental publishing** — add `publish = false` to root and smoke Cargo.toml
4. **[MEDIUM] Ensure script portability** — use `git rev-parse --show-toplevel` in the script