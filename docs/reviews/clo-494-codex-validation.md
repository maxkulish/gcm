# Pre-PR validation: clo-494

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-06-22
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

- MEDIUM: Project tracking docs were collapsed/staled outside the CLO-494 design scope. [docs/PROJECT.md](/Users/mk/Code/gcm--feat-clo-494-anthropic/docs/PROJECT.md:9) still says CLO-494 is in `Discovery`, [docs/ROADMAP.md](/Users/mk/Code/gcm--feat-clo-494-anthropic/docs/ROADMAP.md:17) removes most roadmap tasks, and [docs/DEPENDENCIES.md](/Users/mk/Code/gcm--feat-clo-494-anthropic/docs/DEPENDENCIES.md:9) now says there are no blockers/ready tasks. This is unrelated to the provider implementation and risks damaging the repo's planning source of truth.

- MEDIUM: Several plan-required verification points are missing. The plan requires a `GCM_ANTHROPIC_BASE_URL` override test and provider-selection/error tests, but [src/provider/anthropic.rs](/Users/mk/Code/gcm--feat-clo-494-anthropic/src/provider/anthropic.rs:497) only tests the default base URL, and [src/provider/mod.rs](/Users/mk/Code/gcm--feat-clo-494-anthropic/src/provider/mod.rs:376) does not assert `pick_provider_id(None, Some("anthropic"))` or that the bogus-provider error lists `anthropic`. The acceptance harness also has no Anthropic/mock header coverage despite the plan calling for header verification.

- LOW: README provider/privacy docs are stale. [README.md](/Users/mk/Code/gcm--feat-clo-494-anthropic/README.md:10), [README.md](/Users/mk/Code/gcm--feat-clo-494-anthropic/README.md:18), and [README.md](/Users/mk/Code/gcm--feat-clo-494-anthropic/README.md:75) still list only Groq, Google, and OpenAI, even though CLI help now exposes Anthropic and `ANTHROPIC_API_KEY`.

## Missing Items

- No committed Anthropic acceptance/mock test proving `x-api-key` plus `anthropic-version` are sent.
- No unit test for `GCM_ANTHROPIC_BASE_URL` override.
- No unit test for `GCM_PROVIDER=anthropic` selection through `pick_provider_id`.
- No unit assertion that unknown-provider errors list `anthropic`.
- Live Anthropic API assumptions remain unverified here: valid typed plan, forced tool call behavior, no reasoning leakage, and model behavior.

## Recommendations

- Restore or carefully update `docs/PROJECT.md`, `docs/ROADMAP.md`, and `docs/DEPENDENCIES.md` instead of replacing the broader project state.
- Add Anthropic mock coverage to `scripts/acceptance.sh` for missing key, auth/version headers, model override, and forced tool-use response parsing.
- Update README provider/config/privacy tables to include Anthropic.
- I verified `cargo fmt --check`; I did not independently run `cargo clippy` or `cargo test` because this session is read-only and those commands write build artifacts.
- I spot-checked official Anthropic docs: the models table lists `claude-haiku-4-5` as the Claude API alias, and the tool-use docs describe client tools returning `stop_reason: "tool_use"` with `tool_use` blocks: https://platform.claude.com/docs/en/about-claude/models/overview and https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview.
