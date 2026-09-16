# Pre-PR validation: clo-799

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-09-16
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

MEDIUM: [src/config/mod.rs](/Users/mk/Code/gcm--fix-clo-799-model-pining/src/config/mod.rs:535) adds `pub fn known_models_not_enabled(...)` under `gcm::config`, and [src/lib.rs](/Users/mk/Code/gcm--fix-clo-799-model-pining/src/lib.rs:26) publicly exports `config`. That makes this helper externally callable despite `#[doc(hidden)]`. The design explicitly says "No public library API changes"; make this helper private unless an external consumer really needs it.

LOW: [tests/provider.rs](/Users/mk/Code/gcm--fix-clo-799-model-pining/tests/provider.rs:197) only asserts the new Google test is not `Config`. That can pass for the wrong reason, such as `OnboardingRequired` or `MissingKey`, and would not prove the run reached provider transport as the comment claims. Assert the expected transport/reachability error code or at least assert it is not onboarding/missing-key and that the status fails.

## Missing Items

No AC1-AC3 implementation gap found in the code path itself. The runtime changes match the design: rejection messages include known models, the wizard note/prompt was added, and `initial_default_model` now prefers the shipped default when selected.

## Recommendations

Make `known_models_not_enabled` a private `fn`; the existing same-module tests can still call it.

Tighten `newly_enabled_model_passes_enforcement` to prove it reached the closed Google endpoint.
