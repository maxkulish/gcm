# Dependencies - gcm

**Last Updated**: 2026-07-12 (CLO-555 started; CLO-554 blocked by it)

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| CLO-554 | CLO-555 | In Progress | Rebase loop builds on the transaction engine + `StoppedOnNextConflict` handoff |

## Unblocked & Ready

| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| CLO-547 | split from CLO-545 review (self-contained, no blockers) | 2026-07-11 |

> **CLO-555** (rework `gcm resolve` into an ownership transaction) is In Progress (spec phase); **CLO-554** (rebase resolve-until-clean loop) waits on it. **CLO-545** (OpenAI GPT-5.6 model refresh) merged in PR #34 (2026-07-11); the owner's live API smokes (AC7, need `OPENAI_API_KEY`) are the only remaining step. **CLO-547** (provider-wide model-discovery hardening, split from the CLO-545 review) is ready with no blockers. **CLO-537** (Vertex AI provider, keyless ADC) merged in PR #32 (2026-07-09) — the only remaining step is the maintainer's live ADC end-to-end check (**HITL**). All prior tracked gcm work (CLO-485…CLO-535) is Done; CLO-533 (`gcm resolve` remote MR/PR orchestration, Phase 2) merged in PR #30.

## Recently Resolved Blockers

| Task | Previous Blocker | Resolved |
|------|-----------------|----------|
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
