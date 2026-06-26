# Dependencies - gcm

**Last Updated**: 2026-06-26 (CLO-515 `gcm status` command merged PR #21, Done — first v2 slice, had no open blockers (read config plumbing from CLO-485/493/496, all Done), blocked nothing. CLO-497 cross-platform releases + alias cutover merged PR #20, Done — the LAST v1 slice; the bash→Rust migration is complete. CLO-514 secret-scanner rule-pack + entropy engine merged PR #18, Done — new FR-60, hardens FR-50. CLO-496 first-run onboarding wizard merged PR #17, Done; CLO-490 secret scanning + gcmignore merged PR #16, Done; CLO-488 finalized to Done — PR #6 merged 2026-06-21, `9052a7e`; CLO-494 merged PR #11; CLO-495 merged PR #14; CLO-493 merged PR #12; CLO-489 merged PR #10; CLO-492 PR #9; CLO-491 merged on main)

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| — | — | — | None. CLO-497 unblocked 2026-06-22 (CLO-496 merged) + 2026-06-23 (CLO-514 merged) |

## Unblocked & Ready

| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| — | None — all v1 slices (CLO-485…CLO-497) + CLO-514 Done | — |

## Recently Resolved Blockers

| Task | Previous Blocker | Resolved |
|------|-----------------|----------|
| CLO-497 | CLO-487…CLO-496 (all Done) | 2026-06-24 (merged PR #20) |
| CLO-514 | CLO-490 (Done 2026-06-22, PR #16) | 2026-06-23 (merged PR #18) |
| CLO-497 | CLO-496 (Done 2026-06-22, PR #17) | 2026-06-22 (last dependency cleared) |
| CLO-488 | CLO-486 (Done 2026-06-19) | 2026-06-21 (merged PR #6); finalized to Done 2026-06-22 |
| CLO-490 | CLO-486 (Done 2026-06-19) | 2026-06-22 (merged PR #16) |
| CLO-494 | CLO-489 (Done 2026-06-21) + CLO-485 (Done 2026-06-19) | 2026-06-22 (merged PR #11) |

> CLO-514 (secret-scanner rule-pack + entropy engine) merged PR #18 (squash) 2026-06-23 → Done, new FR-60, hardens FR-50. CLO-497 (release + cutover) is now the only remaining slice and is fully unblocked. CLO-496 (onboarding wizard) merged PR #17 (squash) 2026-06-22 → Done, clearing the last dependency on CLO-497. The entire v1 feature set (CLO-485…CLO-496 + CLO-514) is complete. CLO-490 merged 2026-06-22 (PR #16, Done) — secret scanning + `gcmignore`. CLO-488 (typed errors + retries) PR #6 merged 2026-06-21 (`9052a7e`), finalized to Done 2026-06-22. CLO-494 merged 2026-06-22 (PR #11, Done) — Anthropic. CLO-495 merged 2026-06-22 (PR #14, Done) — Ollama. CLO-493 merged 2026-06-22 (PR #12, Done). CLO-489 merged 2026-06-21 (PR #10, `ca1db75`, Done). CLO-491 + CLO-492 merged 2026-06-21.
