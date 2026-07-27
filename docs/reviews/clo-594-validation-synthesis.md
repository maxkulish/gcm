# Pre-PR validation: clo-594

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-27
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | FAIL verdict, 3 findings (2 HIGH, 1 MEDIUM), 3 missing items. All 3 independently verified against the files. |
| Gemini | OK | PASS verdict, 3 findings all LOW/informational. Confirmed the docs-only claim (`git diff main...HEAD -- '*.rs'` = 0 lines) but did not read ADR §1/§3/§Consequences against §11, so it missed the internal contradiction it had itself flagged upstream. |
| Claude fallback | SKIPPED | At least one external reviewer succeeded. |

## Verdict

PASS_WITH_NOTES

## Must Fix Before PR

All three are verified defects in the deliverable itself (the ADR is the artifact CLO-595 will implement from), and all three are bounded documentation edits in two files.

**1. ADR self-contradicts on type identity — the central boundary mechanism.** Confirmed. `docs/adrs/002-library-boundary.md:46`, `:55`, `:100` and `:220` all state the binary keeps `use crate::config::Config` and is "unchanged", while `:228` (Implementation Notes §1) states the opposite: the binary must import from the library crate (`use gcm::config::Config`) or the compiler treats the two as distinct types. The design doc agrees with `:228` (`docs/designs/clo-594-library-boundary.md:56`, `:98`). Under the ADR's own `lib.rs` sketch (`design:85`, `pub use crate::config::...`), both targets declare `mod config;`, so the source compiles twice into two incompatible `Config` types. An implementer following §1/§3 literally hits a type mismatch the first time the binary passes a `Config` to anything reached through `gcm::`. Fix: rewrite `:46`, `:55`, `:100`, `:220` to say the binary imports shared types from the library target, and note in Decision 1's alternatives (`:49`) that the chosen option still requires migrating shared-type imports — just not every module — so the "minimal disruption" driver stays honest.

Related, same fix pass: `design:98` says the binary "continues to declare `mod config;` … but imports from the library crate" — that combination is exactly the double-compilation the assumption A1 warns against.

**2. `clap` decision omits `AutoPolicy` and the feature snippet is miswired.** Both halves confirmed. `AutoPolicy` derives `clap::ValueEnum` at `src/config.rs:138` and is in the exported surface (`adr:194`, `design:85`), but Decision 4 names only `SecretScanMode` and `ProviderId` and asserts "two sites to maintain" (`adr:123`). Third site is real. Separately, `design:65-69` sets `default = ["cli"]`, `cli = ["dep:clap", …]`, `clap = ["dep:clap"]` — `dep:clap` activates the optional dependency without activating the `clap` *feature*, so under default features `#[cfg_attr(feature = "clap", derive(ValueEnum))]` evaluates false and the binary's `value_enum` args fail to compile. Fix: add `AutoPolicy` to Decision 4 and correct "two sites" to three; change `cli` to `["clap", "dep:cliclack", "dep:console"]`.

**3. The consumer dependency name is wrong as written.** Confirmed. The package is `gcm` (`Cargo.toml:2`) and Decision 1 explicitly rejects a separate crate, yet the ADR calls the library `gcm-core` at `:112`, `:169`, `:188`, `:191` and gives `gcm-core = { path = "../gcm" }` (`:169`) as the dependency form — Cargo resolves that path expecting a package named `gcm-core` and errors. Fix: use `gcm = { path = "../gcm" }`, or `gcm-core = { package = "gcm", path = "../gcm" }` if the alias is wanted, and make the naming consistent throughout.

## Out of Scope / Deferred

- **Status is `Accepted` before human review.** `adr:3` says Accepted, but the ADR's own rollout says commit as Proposed and flip to Accepted after review (`design:159-161`), while `plan:6` already records Accepted. Process nit, not a correctness defect; the PR review *is* the human review. Leave as-is or flip during the fix pass — either is fine.
- **Gemini's CI recommendation** (test both `cargo test` and `cargo test --no-default-features`). Correct and already captured as an ADR consequence (`adr:124`) and an edge case (`design:150-153`). Belongs to CLO-595, not this branch.
- **`GcmError` decoupling, `#[cfg(unix)]` on the `0600` write.** Already recorded as deferred implementation notes (`adr:231-232`, assumptions A4/A5). Correctly deferred.

## False Positives / Tooling Artifacts

- **Gemini's overall PASS.** Not a tooling artifact — its factual claims check out (zero `.rs` changes, all 7 decisions carry chosen/rejected/reason, implementation notes present). It simply reviewed the ADR for *presence* of decisions rather than *internal consistency* across them, so it read §11 in isolation and never cross-checked it against §1/§3/§Consequences. Its conclusion is superseded, its evidence is not.
- Nothing in the Codex report was a false positive. All three findings reproduce at the cited line numbers.

## Recommendation

PROCEED_WITH_FIXES. The branch is structurally sound: it is genuinely docs-only, all seven decisions carry a chosen option, a rejected alternative, and a reason, and the Library Surface section answers "where does `Config`/`ProviderId`/`SecretScanMode` live" cleanly — so acceptance criteria 1, 3 and 4 are met and nothing here calls for a scope pivot or a user decision. What fails today is criterion 2 in its stronger reading: a reader following ADR-002 alone would build the wrong thing, because the document tells them both to keep `use crate::X` and to switch to `use gcm::X`, undercounts the `ValueEnum` sites by one, and hands them a Cargo snippet that breaks the default build plus a dependency line that will not resolve. Those are four localized edits across two files — `docs/adrs/002-library-boundary.md` lines 46/49/55/100/108/123/169/188/191/220 and `docs/designs/clo-594-library-boundary.md` lines 65-69/98 — well inside one bounded fix iteration, with no code to touch and no test suite to re-run. Apply them, re-read Decision 1 and Decision 4 end-to-end for consistency, then open the PR; do not carry the contradiction into CLO-595, since that ticket is scheduled to implement directly from this text.
