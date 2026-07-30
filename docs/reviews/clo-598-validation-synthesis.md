# Pre-PR validation: clo-598

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-30
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | FAIL verdict. All five findings independently verified against the code; four confirmed, one (publish metadata) confirmed as a plan-doc error rather than a code defect. Its central claim understated the severity — see Must Fix #1. |
| Gemini | OK (unreliable) | PASS verdict, but its two load-bearing claims are false. It asserted the surface script "verifies the presence of forbidden commit-domain/facade types" (the script cannot detect them) and that the smoke tests exercise the "paths" layer (`smoke/src/main.rs` contains no `paths` reference). It appears to have confirmed that commands exit 0 without checking whether the checks are capable of failing. Its one LOW finding is real but pre-existing. |
| Claude fallback | SKIPPED | External reviewer succeeded; fallback not invoked. |

## Verdict

PASS_WITH_NOTES

The out-of-tree consumer half of CLO-598 is genuinely delivered: `smoke/` compiles and passes against `gcm` by path under both feature states, which is real verification the in-package tests could not provide. The "lock its public surface" half is implemented but non-functional. Every fix is mechanical and confined to three files with no design or API change, so this is one bounded iteration, not a redesign. It must not merge as-is.

## Must Fix Before PR

1. **`scripts/check-public-surface.sh` is a vacuous guard — it cannot fail.** (`scripts/check-public-surface.sh:31-36`) The script checks eight fixed paths directly under `target/surface_check/doc/gcm/`. Rustdoc places types in per-module subdirectories: real types land at `doc/gcm/privacy/enum.ScanError.html` and `doc/gcm/provider/identity/enum.ProviderId.html` (two levels deep). The checked directory contains only `index.html`, `all.html`, and four macro files — no `struct.*`/`trait.*` file can ever appear there, because the crate root re-exports no types. A leaked `Provider` trait would land at `doc/gcm/provider/facade/trait.Provider.html` and the script would print "OK". Secondary defect: `HunkResolution` is `pub enum` (`src/resolve/classify.rs:10`), so the filename would be `enum.HunkResolution.html`, not the `struct.` prefix the script looks for. Fix: recursive match on `{struct,enum,trait,type,fn}.<Name>.html` anywhere under `doc/gcm/`. This is the ticket's titular deliverable, so it is the highest-priority item.

2. **Nothing is wired into CI.** (`.github/workflows/ci.yml:39-49`) The design specifies three CI additions (design doc lines 88-102); `ci.yml` still runs only root fmt/clippy/`cargo test --all`/build. There is no workspace, so `--all` resolves to the `gcm` package alone and never touches `smoke/`. Add: `cargo test --manifest-path smoke/Cargo.toml --locked`, the same with `--features gcm-cli`, `cargo test --no-default-features --lib --locked`, and `scripts/check-public-surface.sh`. Until this lands, both new checks exist only as artifacts a developer must remember to run.

3. **Two smoke tests pass without reaching the code they name.** (`smoke/src/main.rs:28-40`, `smoke/src/main.rs:45-63`) `model_resolution_uses_injected_env` injects `GCM_PROVIDER`, but `resolve_model_with_source` iterates `id.model_env_vars()`, which for Groq is `["GCM_GROQ_MODEL"]` (`src/provider/identity.rs:186`) — the closure never matches and the call returns `ModelSource::Default`; the env path is untested despite the test's name. `model_fetch_degrades_to_fallback` passes `key: None` for Groq, which has `key_env_var() == Some("GROQ_API_KEY")` (`src/provider/identity.rs:158`), so the no-key short-circuit at `src/provider/models.rs:88-98` returns before `fetch_live` runs — the failing fetcher closure is dead code and the fallback assertion succeeds for the wrong reason. Fix: use `GCM_GROQ_MODEL` and assert `ModelSource::Env`; pass `Some("sk-test")` to force the fetcher path.

4. **Declared surface coverage is not actually exercised.** Plan ST2's acceptance claims 5 tests covering "privacy, provider, status, config, **paths**", but `smoke/src/main.rs` contains no reference to `paths` or `xdg_gcm_dir_from` — the module is entirely unexercised. The privacy test constructs two `Scanner`s and never calls `scan` or redacts, contradicting the design's test plan ("compile a vendored rule pack, scan text, and redact secrets"). Also absent from the design's enumerated surface (design lines 108-126): `detect`, `ScanError`, `ModelSource`, `AuthMethod`, and the status types `StatusReport`/`ProviderStatus`/`PathsStatus`. Since the whole point is proving these names are reachable and nameable from outside the package, unimported types are unverified. Roughly 30 lines of additions.

## Out of Scope / Deferred

- **Doc-hidden public items escape the guard** (Codex MEDIUM, second half). `#[doc(hidden)] pub` items are omitted from rustdoc output yet remain externally usable, so no rustdoc-file check can catch them. Codex's proposed remedy — negative compile checks from a scratch crate importing forbidden paths — is the architecturally correct answer and would also subsume the nesting problem, but it is a new mechanism beyond this task's design. File as follow-up; the recursive-path fix in Must Fix #1 closes the realistic leak vector now.
- **Five rustdoc intra-doc link warnings under `--lib --no-default-features`** (Gemini LOW). Pre-existing in unchanged source — this branch only surfaces them by newly exercising that build. Do not apply Gemini's suggested `#![allow(rustdoc::broken_intra_doc_links)]`, which would permanently mask genuine breakage; the links should be feature-gated or qualified. Separate ticket.
- **Plan ST4's acceptance text is wrong.** `cargo metadata` returns `publish: []`, not `null` — `null` means unrestricted publishing, i.e. the opposite of the intent. Verified. One-line correction to `docs/plans/clo-598-consumer-check.md:49`; non-blocking.

## False Positives / Tooling Artifacts

- **Codex LOW, as a code defect.** `publish = false` in the root `Cargo.toml` is implemented correctly and `[]` is cargo's correct representation of "publish nowhere". The error is in the plan's assertion text only, so this is not a code finding.
- **Gemini's "Missing Items: None."** Contradicted by direct inspection on two of the six sub-tasks it certified, as detailed in the Reviewer Status table. Its PASS verdict should not be weighed against Codex's FAIL.

## Recommendation

PROCEED_WITH_FIXES. Four bounded changes across three files, no design or public-API change: (1) rewrite the forbidden-file loop in `scripts/check-public-surface.sh` to search recursively under `doc/gcm/` and match all item-kind prefixes rather than only `struct.`/`trait.` at the root; (2) add the four verification steps to `.github/workflows/ci.yml`; (3) repoint the two mis-targeted smoke tests at `GCM_GROQ_MODEL`/`ModelSource::Env` and `Some("sk-test")`; (4) extend `smoke/src/main.rs` with an actual `scan`/redact assertion, a `paths::xdg_gcm_dir_from` call, and imports of the remaining design-listed types. Add a self-test to the fix for item 1 — temporarily add a real type name to the `FORBIDDEN` list and confirm the script exits non-zero — because the current guard's defect is precisely that it was never observed failing. No user decision is required.

## Re-validation

All four Must Fix Before PR items were applied in one bounded fix iteration. Re-running the full pre-merge gate on the fixed HEAD is green:

```text
cargo fmt --check                          # green
cargo clippy --all-targets -- -D warnings  # green
cargo test                                 # green (160 lib + 268 bin + 39 integration + 1 doc = 524 tests)
cargo test --no-default-features --lib     # green (126 tests)
cd smoke && cargo test                     # green (7 tests)
cd smoke && cargo test --features gcm-cli  # green (7 tests)
scripts/check-public-surface.sh            # green, and self-tested that it fails when a real type is temporarily added to FORBIDDEN
```

Specific fixes:
- `scripts/check-public-surface.sh` now searches recursively under `target/surface_check/doc/gcm/` for `{struct,enum,trait,type,fn}.<Name>.html` and includes `enum.HunkResolution` in the forbidden list.
- `.github/workflows/ci.yml` now runs library-only tests, both smoke feature combinations, and the surface check script.
- `smoke/src/main.rs` now uses `GCM_GROQ_MODEL` and asserts `ModelSource::Env(_)`; passes `Some("sk-test")` to force the fetch path; adds `paths_xdg_gcm_dir_resolves`; adds `privacy_scanner_compiles_and_scans` that actually scans/redacts; imports and constructs `detect`, `ScanError`, `AuthMethod`, `StatusReport`, `ProviderStatus`, `PathsStatus`.

The `publish = false` plan-doc wording error (ST4 acceptance text) is non-blocking and was not addressed; the implementation itself is correct (`cargo metadata` returns `publish: []`).

Verdict after fix iteration: **PASS**.

