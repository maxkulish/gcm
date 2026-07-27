# Lessons: CLO-594 — Lock the gcm library boundary (ADR)

## L1 — An ADR that says "binary keeps `use crate::X`" while also saying "binary must import from the library crate" will confuse implementers

**Source incident:** CLO-594 validation gate. Codex caught that ADR-002
contradicted itself on type identity: Decision 1/3/Consequences said the
binary keeps `use crate::config::Config` (4 sites), while Implementation
Notes §1 said the binary must import `use gcm::config::Config` or the
compiler treats them as distinct types. The design doc agreed with
Implementation Notes. An implementer following §1/§3 literally hits a
type mismatch the first time the binary passes a `Config` to anything
reached through `gcm::`.

**Rule:** In a single-package `[lib]+[[bin]]` setup, the binary must
import shared types from the library target (`use gcm::config::Config`),
not declare `mod config;` in both targets. Both targets compile as
separate crates; `crate::Config` and `gcm::Config` are distinct,
incompatible types. An ADR that locks this boundary must state this
consistently in every decision section, not just in an implementation
notes appendix.

**How to apply:** When writing or reviewing an ADR that adds a `[lib]`
target, grep for `use crate::` in the ADR and verify each mention is
either (a) binary-internal modules that stay as `mod` declarations or
(b) explicitly noted as needing migration to `use gcm::X`. Do not leave
both "keeps `use crate::X`" and "imports from library" in the same
document.

## L2 — `dep:clap` in a Cargo feature activates the optional dependency but does NOT activate the `clap` feature

**Source incident:** CLO-594 validation gate. The design doc's
Cargo.toml snippet had `cli = ["dep:clap", "dep:cliclack",
"dep:console"]` and `clap = ["dep:clap"]`. Under default features
(`default = ["cli"]`), `dep:clap` makes the `clap` crate available but
does not enable the `clap` *feature*, so
`#[cfg_attr(feature = "clap", derive(ValueEnum))]` evaluates false and
the binary's `value_enum` args fail to compile.

**Rule:** In Cargo features, `dep:clap` and `clap` are different:
`dep:NAME` activates the optional dependency without enabling any
feature named `NAME`. If a `cfg_attr(feature = "clap", ...)` gate
exists, the feature that enables it must include `"clap"` (not just
`"dep:clap"`) in its dependency list. Use `cli = ["clap",
"dep:cliclack", "dep:console"]` so the `cli` feature enables both the
dependency and the `clap` feature.

**How to apply:** When writing Cargo.toml feature snippets for optional
dependencies with `cfg_attr` gates, verify that the feature name in
`cfg_attr(feature = "X")` appears (without the `dep:` prefix) in the
enabling feature's dependency list. Test with `cargo build
--no-default-features --features cli` to confirm the gate fires.

## L3 — The package name in a path dependency must match `[package] name`, not the `[lib]` target name

**Source incident:** CLO-594 validation gate. The ADR called the
library `gcm-core` in 5 places and gave `gcm-core = { path = "../gcm"
}` as the consumer dependency form. But the package is named `gcm`
(`Cargo.toml:2`), and Decision 1 explicitly rejects a separate crate.
Cargo resolves `gcm-core = { path = "../gcm" }` expecting a package
named `gcm-core` and errors.

**Rule:** In a single-package `[lib]+[[bin]]` setup, the package name
is the one in `[package] name = "..."`. Consumers depend on it via
`gcm = { path = "../gcm" }` (or `gcm-core = { package = "gcm", path =
"../gcm" }` if an alias is wanted). Do not invent a different name for
the library target — the `[lib]` target inherits the package name
unless explicitly renamed with `[lib] name = "..."`.

**How to apply:** When writing consumer dependency examples in an ADR
or design doc, verify the package name matches `Cargo.toml`'s
`[package] name` field. If a different library name is desired, use
`[lib] name = "gcm-core"` in `Cargo.toml` and reference that name
consistently. Otherwise use the package name everywhere.