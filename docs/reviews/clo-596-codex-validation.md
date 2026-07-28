# Pre-PR validation: clo-596

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-28
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

- MEDIUM: OpenAI supported models now have two sources of truth. The library/default/registry uses `OPENAI_SUPPORTED_MODELS` in [identity.rs](/Users/mk/Code/gcm--feat-clo-596-provider-id/src/provider/identity.rs:145) and [models.rs](/Users/mk/Code/gcm--feat-clo-596-provider-id/src/provider/models.rs:351), while runtime validation still uses `SUPPORTED_MODELS` in [openai.rs](/Users/mk/Code/gcm--feat-clo-596-provider-id/src/provider/openai.rs:23). They match today, but this directly reintroduces the drift CLO-596 is trying to prevent.

- MEDIUM: The split dropped runtime selection tests from `main`. `select()` and `pick_provider_id()` are still central runtime seams in [facade.rs](/Users/mk/Code/gcm--feat-clo-596-provider-id/src/provider/facade.rs:236), but the facade test module now only covers response/schema helpers starting at [facade.rs](/Users/mk/Code/gcm--feat-clo-596-provider-id/src/provider/facade.rs:428). The stale comment in [openai.rs](/Users/mk/Code/gcm--feat-clo-596-provider-id/src/provider/openai.rs:289) still refers to a `mod.rs` select-gate test that no longer exists.

- LOW: `git diff --check` fails due trailing whitespace in [docs/discovery/clo-596.md](/Users/mk/Code/gcm--feat-clo-596-provider-id/docs/discovery/clo-596.md:3) and [docs/prds/clo-596-expose-provider-identity-and-registry.md](/Users/mk/Code/gcm--feat-clo-596-provider-id/docs/prds/clo-596-expose-provider-identity-and-registry.md:3).

## Missing Items

- `ProviderId::auth_method()` is implemented in [identity.rs](/Users/mk/Code/gcm--feat-clo-596-provider-id/src/provider/identity.rs:223), but the ST6/design-required test for all providers is missing.
- CLI alias acceptance is not directly tested. The library parser tests cover `gemini` and `google-vertex`, but `--provider` uses Clap via [cli.rs](/Users/mk/Code/gcm--feat-clo-596-provider-id/src/cli.rs:88).
- I could not run Cargo checks: the sandbox is read-only and Cargo failed creating a target dir.

## Recommendations

- Make OpenAI supported models a single shared source, then have both `ProviderId::default_model`/registry filtering and `openai::validate_model` consume it.
- Restore the removed `pick_provider_id_*`, `select_ollama_is_key_free`, and `select_openai_validates_*` tests in `facade.rs`.
- Add `Cli::try_parse_from` tests for `--provider gemini` and `--provider google-vertex`.
- Strip the trailing whitespace before merge.
