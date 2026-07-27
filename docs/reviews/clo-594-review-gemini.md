
> build · gemini-3.5-flash

✱ Glob "docs/adrs/002-library-boundary.md" 1 match
✱ Glob "docs/adrs/001-foundational-architecture-decisions.md" 1 match
✱ Glob "docs/discovery/clo-594.md" 1 match
✱ Glob "docs/ROADMAP.md" 1 match
✱ Glob "docs/DEPENDENCIES.md" 1 match
✱ Glob "docs/PROJECT.md" 1 match
→ Read docs/adrs/002-library-boundary.md
→ Read docs/adrs/001-foundational-architecture-decisions.md
→ Read docs/discovery/clo-594.md
→ Read docs/ROADMAP.md
→ Read docs/DEPENDENCIES.md
→ Read docs/PROJECT.md
## 1. Completeness Check
The design document is exceptionally thorough and covers all key structural and operational aspects of the library boundary. It includes a clear context section, a comprehensive decision summary matrix, detailed rationales for each of the seven key decisions, explicit alternatives considered, and an exhaustive list of short-term and long-term consequences. The proposed library surface is clearly contrasted against the binary-only surface, and evolutionary thresholds (e.g., when to migrate to a full cargo workspace or when to abstract transport behind a trait) are explicitly defined. 

## 2. Architecture Assessment
**Strengths**:
- **Incremental and Minimalist Crate Shape**: Selecting an in-package `[lib]` target instead of a multi-crate workspace reduces migration risk, avoids path-dependency complexities, and preserves the primary user's daily commit workflow intact.
- **Clean Sync/Async Seam**: Realizing that the shared surface consists of pure data types and pure functions - rather than transport, HTTP clients, or provider integrations - elegantly sidesteps a complex, high-risk async rewrite of the binary's HTTP client.
- **Zero-Dependency Library Core**: Offloading CLI-centric concerns by placing the `ValueEnum` derives behind an optional `clap` feature ensures that non-CLI downstream consumers (such as background agents or memory pipelines) can import the library without inheriting heavy CLI arguments parsing overhead.
- **Consistent Architecture Governance**: The proposal successfully replicates the single-ADR locking pattern pioneered in ADR-001, ensuring that critical boundaries are agreed upon before any code extraction is performed.

**Concerns**:
- **Type Divergence and Compile Duplication**: The proposal states that "the binary continues to use `use crate::config::Config` internally" while the library re-exports it. In Rust, a binary and a library in the same package are compiled as separate crates. If both compile the same files via local module declarations (e.g., `mod config;` in both `main.rs` and `lib.rs`), they will compile the code twice, and more importantly, the compiler will treat `gcm::config::Config` and `crate::config::Config` as completely distinct, incompatible types. The binary must depend on the library and import the shared types from it (e.g., `use gcm::config::Config;`) to ensure type identity.
- **Terminal Dependency Bloat**: Gating only `clap` is insufficient. Heavy interactive terminal dependencies like `cliclack` and `console` must also be made optional and feature-gated (e.g., under a `cli` feature enabled by default for the binary). If they remain as non-optional dependencies in the shared `Cargo.toml`, Cargo will download and compile them for any downstream library consumer.

## 3. ADR Compliance
The document is fully compliant with all architectural precedents set in ADR-001.
- **Sync Runtime Compatibility**: Keeps gcm's transport synchronous (`ureq`), preserving the synchronous `Provider` trait boundary and satisfying ADR-001 Decision 2.
- **Config Integrity**: Keeps config location, TOML format, and the `flag > env > config > default` precedence chain completely intact, fulfilling ADR-001 Decision 4.
- **Secrets Protocol**: Reaffirms that credentials remain inside environment variables rather than plaintext config files (ADR-001 Decision 4).

**Violations**: None found.

## 4. Security Review
The design successfully maintains a secure operational posture.
- **Onboarding and Credential Input Isolation**: The interactive onboarding flow, password masking, and TTY key collection remain strictly in the binary, preventing interactive input security risks from leaking into the library.
- **Secure File Writing**: The library retains `load` and `save` functions, placing the responsibility of enforcing secure file permissions (`0600` equivalent) on the library. 
- **Recommendation**: The implementation of the library's `save` function must explicitly use platform-conditional logic (e.g., using `std::os::unix::fs::PermissionsExt` on Unix) to set `0600` permissions on config files to ensure security on Unix without failing compilation on Windows.

## 5. Implementation Concerns
- **Redundant Module-Level Gating**: The document states that binary-internal modules (like `cache`, `git`, `resolve`, `ui`) must be gated behind `#[cfg(not(feature = "library"))]`. This is unnecessary; in Rust, if these modules are not declared via `mod` statements in `lib.rs`, they are completely ignored during the library build. No module-level `cfg` gating is needed.
- **Monolithic Error Types**: The existing `GcmError` enum contains binary-specific variants (`Git`, `Provider`, `OnboardingRequired`). If the library's exported functions return `Result<_, GcmError>`, the library must carry these binary-specific variants, which couples it to binary domains. The error types should be partitioned or mapped to a library-specific error.

## 6. Blind Spots
- **Windows Compile Support**: Writing config files with `0600` permissions requires Unix-specific file-permission APIs. The design does not detail how Windows compatibility will be preserved during the library extraction of the config module.
- **Cargo Dependency Pollution**: Because the package uses a single `Cargo.toml`, any consumer pulling in the library via a path or git dependency will have their Cargo resolver pull in all of `gcm`'s dependencies. To prevent compilation bloat, all binary-only dependencies (`cliclack`, `console`, `ureq`) must be marked `optional = true` and managed under feature flags.

## 7. Verdict
`APPROVE_WITH_SUGGESTIONS`

## 8. Actionable Feedback
1. **Refactor Binary to Consume the Library (Critical)**: Refactor `main.rs` and binary-only modules to import shared types directly from the library (e.g., `use gcm::config::Config;`) instead of declaring `mod config;` locally in both targets. This guarantees type equivalence and prevents compilation duplication.
2. **Gate Heavy Binary Dependencies in Cargo.toml**: Mark interactive terminal dependencies (`cliclack`, `console`) as `optional = true` in the shared `Cargo.toml`. Create a default `cli` feature for the binary target that enables them, keeping the library build lightweight.
3. **Remove Redundant Module Gates**: Eliminate plans to add `#[cfg(not(feature = "library"))]` to binary-internal files. Simply omit these modules from `src/lib.rs` to keep them out of the library target.
4. **Platform-Conditional Permissions**: Implement Unix-conditional compilation flags (e.g., `#[cfg(unix)]`) for setting `0600` permissions on the written config file in the library's `save` function.
5. **Decouple Error Handling**: Define a lightweight library-specific error type (e.g., `ConfigError` or `LibraryError`), or refactor `GcmError` to decouple the core configuration loading/saving errors from complex binary runtime errors.
