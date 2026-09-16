# Pre-PR validation: clo-799

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-09-16
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | FAIL verdict, 1 MEDIUM + 1 LOW; both verified against the code below |
| Gemini | OK | PASS verdict, 2 positive LOW findings, 1 forward-looking recommendation |
| Claude fallback | SKIPPED | Both external reviewers succeeded |

Independent verification run in the worktree: `cargo fmt --check` clean, `cargo clippy --all-targets --all-features` no warnings, full suite 529 tests across 12 binaries, 0 failures.

## Verdict
PASS

## Must Fix Before PR
- None.

## Out of Scope / Deferred
- **`known_models_not_enabled` visibility (Codex MEDIUM, downgraded).** Making it private is a safe one-word change - all four call sites are in `src/config/mod.rs` (line 574 plus three module tests). It is not a blocker (see False Positives), but if a follow-up touches this module, tightening to a bare `fn` costs nothing.
- **Static fallback catalog drift (Gemini recommendation).** `src/provider/models.rs` hardcodes per-provider model lists that will age as OpenAI/Google ship new defaults. Real, but a separate maintenance concern - it predates this branch and CLO-799 does not ask for a sync mechanism. Worth a follow-up ticket.
- **`newly_enabled_model_passes_enforcement` asserts only `!= "Config"` (Codex LOW).** The concern is fair in the abstract: `OnboardingRequired` or `MissingKey` would also satisfy it. In practice the test writes a config with `key = "k"` and a `default`, so those two codes are excluded by construction, and the neighbouring pre-existing tests (`empty_models_allows_any_model`, `enforcement_runs_on_clean_repo`) use the identical `assert_ne!(error_code, "Config")` shape. Tightening it would be an improvement to the file's whole convention, not a fix to this change.

## False Positives / Tooling Artifacts
- **"Design says no public library API changes" (Codex MEDIUM, the basis of its FAIL).** Codex read `pub fn` under a `pub mod config` and concluded a surface change. It did not check the module's convention: every one of the ~25 helpers in `src/config/mod.rs` - including `model_is_enabled`, the function this helper exists to serve, and `canonicalize_model`, `merge_provider_config`, `build_config` - is declared `#[doc(hidden)] pub fn` on `main` already. The new helper is marked identically. `docs/adrs/002-library-boundary.md` defines the library boundary as which *modules and types* the lib target re-exports, not item-level visibility of doc-hidden helpers, and there is no locked-surface test to break. The design's own architecture section is explicit about what it was guarding: it changed `static_fallback_models` to `pub(crate)` precisely so the catalog would not become public. That constraint was honoured. Nothing documented or externally supported changed, so this does not warrant FAIL.

## Recommendation
PROCEED. The two reviewers disagreed on one point, and on inspection the disagreement resolves in Gemini's favour: Codex's FAIL rests entirely on a convention it did not sample, and its remaining finding is a test-strength nit that the file's existing tests share. AC1, AC2 and AC3 are each implemented at the surface the design named - the rejection message carries the canonicalized known-minus-enabled catalog with a `DISPLAY_CAP` of 8, the wizard emits the SPACE/ENTER note, and `initial_default_model` prefers the shipped default - and the CLO-516 whitelist gate is preserved and still covered by `enforcement_runs_on_clean_repo`. No Must Fix items, no scope divergence, and the build is green on my own run rather than only by report. Open the PR; file the model-catalog-drift item as a follow-up ticket rather than folding it into this branch.
