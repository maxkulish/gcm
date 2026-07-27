# Pre-PR validation: clo-594

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-27
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

HIGH - ADR contradicts the required type-identity boundary.
[ADR-002](/Users/mk/Code/gcm--feat-clo-594-lock/docs/adrs/002-library-boundary.md:46) and [line 220](/Users/mk/Code/gcm--feat-clo-594-lock/docs/adrs/002-library-boundary.md:220) say the binary can keep using `crate::...` imports unchanged, but the same ADR later says shared types must be imported from `gcm::...` to avoid duplicate binary/lib types ([line 228](/Users/mk/Code/gcm--feat-clo-594-lock/docs/adrs/002-library-boundary.md:228)). The design doc also requires `use gcm::config::Config` ([design](/Users/mk/Code/gcm--feat-clo-594-lock/docs/designs/clo-594-library-boundary.md:56)). This leaves the core boundary not actually locked.

HIGH - The `clap` dependency decision misses an exported type and has broken feature wiring.
The ADR says only `SecretScanMode` and `ProviderId` need `ValueEnum` gating, with "two sites to maintain" ([ADR](/Users/mk/Code/gcm--feat-clo-594-lock/docs/adrs/002-library-boundary.md:108)), but `AutoPolicy` is part of the planned library surface ([design](/Users/mk/Code/gcm--feat-clo-594-lock/docs/designs/clo-594-library-boundary.md:85)) and currently derives `clap::ValueEnum` ([src/config.rs](/Users/mk/Code/gcm--feat-clo-594-lock/src/config.rs:138)). Also, the design's feature snippet gates derives on `feature = "clap"` but `default = ["cli"]` only enables `dep:clap`, not the `clap` feature itself ([design](/Users/mk/Code/gcm--feat-clo-594-lock/docs/designs/clo-594-library-boundary.md:65)). Following this recipe can break either `cargo build` or the no-default library build in CLO-595.

MEDIUM - The crate name/path dependency is ambiguous and partly incorrect.
The design declares `[lib] name = "gcm"` ([design](/Users/mk/Code/gcm--feat-clo-594-lock/docs/designs/clo-594-library-boundary.md:61)), but the ADR repeatedly describes the library as `gcm-core` and gives `gcm-core = { path = "../gcm" }` as the dependency form ([ADR](/Users/mk/Code/gcm--feat-clo-594-lock/docs/adrs/002-library-boundary.md:169), [line 188](/Users/mk/Code/gcm--feat-clo-594-lock/docs/adrs/002-library-boundary.md:188)). If the package remains named `gcm`, a renamed dependency needs `package = "gcm"`; otherwise consumers will look for a package named `gcm-core`.

## Missing Items

- ADR-002 does not currently satisfy the acceptance criterion that a reader can tell the final library/binary boundary from the ADR alone.
- The `clap` question is not fully answered for the actual exported config surface because `AutoPolicy` is omitted.
- The exact consumer dependency/import name is not locked.

## Recommendations

- Rewrite the ADR so all shared-type guidance consistently says the binary imports shared types from the library crate, and remove the stale "all `use crate::X` imports continue" language.
- Add `AutoPolicy` to the `clap` feature decision and implementation notes.
- Fix the feature example to use something like `cli = ["clap", "dep:cliclack", "dep:console"]` if derives are gated on `feature = "clap"`.
- Decide whether the crate is consumed as `gcm` or as a renamed dependency, and show the exact Cargo dependency form.
