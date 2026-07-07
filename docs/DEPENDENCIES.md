# Dependencies - gcm

**Last Updated**: 2026-07-07 (synced from Linear: `gcm resolve` Phase 1 shipped; Phase 2 CLO-533 unblocked & ready)

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| — | — | — | None. CLO-533 (resolve Phase 2) was gated on CLO-531 (Phase-1 core), which merged 2026-07-07 (PR #25) → now unblocked. |

## Unblocked & Ready

| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| CLO-533 | CLO-531 (Phase-1 core, Done, PR #25) | 2026-07-07 |

> **CLO-533** (`gcm resolve` remote MR/PR orchestration, Phase 2) is the only open task — Low priority, HITL, unblocked. It is a thin fetch→core wrapper over the CLO-531 engine (`gh`/`glab` on PATH, dedicated resolution branch, opt-in push). All other gcm work (CLO-485…CLO-531 + bug fixes CLO-517/534/535) is Done.

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

> **`gcm resolve` feature (Phase 4):** CLO-531 (Phase-1 local conflict-marker engine) merged PR #25 2026-07-07 → Done, building on the provider trait (CLO-489), structured output (CLO-487), config (CLO-496/516), and secret-scan (CLO-490/514) layers. Two follow-up bugs fixed same-cycle: CLO-534 (Gemini HTTP 400, PR merged) and CLO-535 (trailing-newline splice, PR #29). CLO-533 (Phase 2 remote MR/PR) is now unblocked and is the sole remaining task. **v2 introspection (Phase 3):** CLO-515 (`gcm status`) merged 2026-06-26, CLO-516 (`gcm provider`) merged 2026-06-28 — no open blockers. **Bugfix** CLO-517 (Ollama cloud plan-parse) merged 2026-06-29. The entire v1 feature set (CLO-485…CLO-496 + CLO-514) is complete; the bash→Rust migration finished with CLO-497 (PR #20).
