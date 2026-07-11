# Pre-PR validation: clo-545

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-11
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- MEDIUM - AC5/AC8 are not met. The spec allows only one intentional legacy-model test fixture, but legacy strings remain in two test fixtures: src/provider/openai.rs:292 and src/provider/mod.rs:835. openai.rs also keeps extra legacy/o-series IDs in the rejection loop at line 297. Keep the AC8 fixture in the select test and change the helper-level rejection test to use non-legacy placeholders like unsupported-model / gpt-5.6-sol.

- LOW - git diff --check main...HEAD fails on trailing whitespace in docs/reviews/clo-545-spec-review-gemini.md:13 and repeated nearby review-section lines. This is easy to fix and avoids a common pre-merge hygiene failure.

- LOW - The current workflow metadata still contains stale round-1 wording: docs/status/clo-545-workflow.yaml:22 says "luna default", and line 84 records the fallback assertion in the wrong order. This conflicts with the approved terra-default spec and could mislead later task status review.

## Missing Items

- AC8: not satisfied as written because more than one src/ test fixture carries legacy OpenAI model strings.
- AC5 manual check: production code is clean, but the required "single intentional fixture" test exception is not clean.
- AC7: live OpenAI smokes remain pending; could not run them in read-only sandbox.

## Recommendations

- Consolidate the legacy rejection coverage to one annotated provider::select fixture.
- Strip trailing whitespace in the generated Gemini review doc.
- Refresh docs/status/clo-545-workflow.yaml so current metadata matches terra default and terra-first fallback.
- Re-run cargo test, cargo clippy -- -D warnings, and the three AC7 live smokes outside read-only sandbox. cargo fmt --check passed; cargo test provider_defaults_and_tokens was blocked by read-only access to target/debug/.cargo-lock.
