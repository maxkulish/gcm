# Dependencies - gcm

**Last Updated**: 2026-07-30 (CLO-597 merged — CLO-598 unblocked)

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| — | None blocked | — | CLO-597 merged 2026-07-30, clearing the last Phase-6 dependency |

## Unblocked & Ready

| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| CLO-598 | CLO-597 complete (status resolution + config types exposed through the library) | 2026-07-30 |

> **Phase 6 (library extraction) is 4/5 done.** CLO-597 merged in PR #49 (2026-07-30) — status resolution and the config types now cross the `[lib]` boundary, with `gcm status` output byte-identical to v0.6.0 — leaving **CLO-598** (out-of-tree consumer check, locks the public surface) as the only open slice and the one that closes the phase. **CLO-554** (rebase resolve-until-clean loop) merged in PR #47 (2026-07-28), built on the **CLO-555** transaction engine from PR #35 — **Phase 4 (`gcm resolve`) is complete** and it blocked nothing downstream. **CLO-545** (OpenAI GPT-5.6 model refresh) merged in PR #34 (2026-07-11); the owner's live API smokes (AC7, need `OPENAI_API_KEY`) are the only remaining step. **CLO-547** (provider-wide model-discovery hardening, split from the CLO-545 review) merged in PR #38 (2026-07-22). **CLO-537** (Vertex AI provider, keyless ADC) merged in PR #32 (2026-07-09) — the only remaining step is the maintainer's live ADC end-to-end check (**HITL**). All prior tracked gcm work (CLO-485…CLO-535) is Done; CLO-533 (`gcm resolve` remote MR/PR orchestration, Phase 2) merged in PR #30.

## Recently Resolved Blockers

| Task | Previous Blocker | Resolved |
|------|-----------------|----------|
| CLO-598 | CLO-597 (source-attributed status resolution) | 2026-07-30 (merged PR #49) |
| CLO-597 | CLO-596 (provider identity + model registry) | 2026-07-28 (merged PR #48, released v0.6.0) |
| CLO-596 | CLO-595 (secret scanner as library API) | 2026-07-28 (merged PR #45/#46) |
| CLO-595 | CLO-594 (library boundary ADR) | 2026-07-27 (merged PR #42/#43/#44) |
| CLO-594 | CLO-593 (lok backend extraction, cross-repo) | 2026-07-26 (merged lok PR #61) |
| CLO-554 | CLO-555 (resolve ownership transaction) | 2026-07-13 (merged PR #35); CLO-554 itself merged 2026-07-28 (PR #47) |
| CLO-533 | CLO-531 (Phase-1 resolve core) | 2026-07-07 (merged PR #25) |
| CLO-534 | CLO-531 (resolve feature) | 2026-07-07 (merged same day) |
| CLO-535 | CLO-531 (resolve feature) | 2026-07-07 (merged PR #29) |
| CLO-497 | CLO-487…CLO-496 (all Done) | 2026-06-24 (merged PR #20) |
| CLO-514 | CLO-490 (Done 2026-06-22, PR #16) | 2026-06-23 (merged PR #18) |
| CLO-497 | CLO-496 (Done 2026-06-22, PR #17) | 2026-06-22 (last dependency cleared) |
| CLO-488 | CLO-486 (Done 2026-06-19) | 2026-06-21 (merged PR #6); finalized to Done 2026-06-22 |
| CLO-490 | CLO-486 (Done 2026-06-19) | 2026-06-22 (merged PR #16) |
| CLO-494 | CLO-489 (Done 2026-06-21) + CLO-485 (Done 2026-06-19) | 2026-06-22 (merged PR #11) |

> **`gcm resolve` feature (Phase 4):** CLO-531 (Phase-1 local conflict-marker engine) merged PR #25 2026-07-07 → Done, building on the provider trait (CLO-489), structured output (CLO-487), config (CLO-496/516), and secret-scan (CLO-490/514) layers. Two follow-up bugs fixed same-cycle: CLO-534 (Gemini HTTP 400, PR merged) and CLO-535 (trailing-newline splice, PR #29). CLO-533 (Phase 2 remote MR/PR) merged in PR #30. **v2 introspection (Phase 3):** CLO-515 (`gcm status`) merged 2026-06-26, CLO-516 (`gcm provider`) merged 2026-06-28 — no open blockers. **Bugfix** CLO-517 (Ollama cloud plan-parse) merged 2026-06-29. The entire v1 feature set (CLO-485…CLO-496 + CLO-514) is complete; the bash→Rust migration finished with CLO-497 (PR #20).
