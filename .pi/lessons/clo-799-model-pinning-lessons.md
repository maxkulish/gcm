# CLO-799 lessons - enabled-model set pinning

## L1 - `#[doc(hidden)] pub fn` is this repo's config-helper convention, not a public-surface change

**Source incident**: CLO-799 pre-PR validation. Codex returned FAIL on the
grounds that `known_models_not_enabled` was a new public API item. It is
`#[doc(hidden)] pub fn`, matching every other helper in `src/config/mod.rs`
(`model_is_enabled`, `canonicalize_model`, `merge_provider_config`,
`build_config`, and ~20 more). The synthesis reviewer cross-checked the module
convention and the locked-surface check and classified the finding as a false
positive; verdict PASS.

**Rule**: In gcm, the library boundary is defined by which *modules and types*
the lib target re-exports (ADR-002), not by item-level `pub` on
`#[doc(hidden)]` helpers. A new `#[doc(hidden)] pub fn` inside an existing
lib module does not widen the supported public surface; converting a real
catalog/provider helper from `fn` to `pub(crate)` is the way to keep it out of
the surface entirely.

**How to apply**: When a design adds or touches a helper in `gcm::config`,
`gcm::provider`, `gcm::privacy`, or `gcm::status`, state in the design's Public
API section that the item follows the module's `#[doc(hidden)]` convention (or
is `pub(crate)`), and prove it with `scripts/check-public-surface.sh`. When a
reviewer flags "new public API" without sampling the module, cite the existing
convention rather than expanding scope to hide every helper.

## L2 - `static_fallback_models` is never empty; it always inserts `default_model()`

**Source incident**: CLO-799 test failures. Three new unit tests assumed
Ollama's static catalog was empty (its `curated` list is `&[]`) and asserted
"no known-models clause". `static_fallback_models` unconditionally inserts
`id.default_model()` at index 0 for every provider, so Ollama's catalog is
`["gemma4:e4b-mlx"]`, not `[]`. The tests failed until they used a catalog
fully covered by the enabled set instead.

**Rule**: `crate::provider::models::static_fallback_models(id)` always contains
at least the provider's `default_model()`. Any test or message that keys on an
"empty catalog" must not assume a provider with no curated entries has an empty
list; construct the empty-known condition by enabling every catalog entry.

**How to apply**: When testing the known-catalog message or the wizard's
candidate merge, assert against the actual catalog contents (`default_model()`
is always present) rather than assuming `curated == []` means `catalog == []`.
When adding a provider, remember the default is injected automatically.

## L3 - Aggregation-file edits in sibling worktrees race at PR health time

**Source incident**: CLO-799 PR #59. A sibling docs/meta commit (CLO-798)
landed on `main` after the branch was cut, making the *same*
PROJECT.md/ROADMAP.md/DEPENDENCIES.md "start" edits. The PR health gate caught
`mergeable: CONFLICTING` / `mergeStateStatus: DIRTY`; the fix was to merge
`origin/main`, keep the newer side's Active Work wording, re-run the pre-merge
gate, and push.

**Rule**: PROJECT.md/ROADMAP.md/DEPENDENCIES.md are shared mutable state across
concurrent worktrees. A sibling task starting or finishing will conflict with
this branch's project-sync edits even when no code overlaps. The conflict is
expected, not a mistake in the branch.

**How to apply**: Run the `pr.md` Step 2.5 PR health gate before waiting on CI
or bots; on `CONFLICTING`/`DIRTY`, `git fetch origin main && git merge
origin/main`, prefer `origin/main`'s wording for aggregation files (it is the
newer base), re-run `cargo fmt --check && cargo clippy --all-targets -- -D
warnings && cargo test`, push, and restart the CI/bot gates. Do not hand-resolve
by discarding the sibling's changes.
