# Dependencies - gcm

**Last Updated**: 2026-09-17 (CLO-802 in review in PR #64; CLO-797 still open)

## Current Blockers

| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| — | None blocked | — | CLO-797 is independent of all Done work |

## Unblocked & Ready

| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| [CLO-797](https://linear.app/cloud-ai/issue/CLO-797) | No dependencies; root cause measured and fix directions written | 2026-09-16 |
| [CLO-800](https://linear.app/cloud-ai/issue/CLO-800) | No dependencies; one-line cause in `merge_provider_config` | 2026-09-16 |
| [CLO-801](https://linear.app/cloud-ai/issue/CLO-801) | No dependencies; approach written against the three wire schemas | 2026-09-16 |
| [CLO-802](https://linear.app/cloud-ai/issue/CLO-802) | No dependencies; cause isolated to `initial_default_model` and reproduced via PTY | 2026-09-17 |

> **One open bug left of the three (2026-09-16).** CLO-797 (the grouping prompt overflows the context
> window) is still open; it came out of the same investigation of a 515-file commit as CLO-798, see
> [investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md](investigations/2026-09-16-clo-797-grouping-prompt-context-overflow.md).
> CLO-798 merged in PR #60 (2026-09-16) and CLO-799 in PR #59. The sequencing note that shaped CLO-798
> held: CLO-801 (return group assignments by index) would remove the ~36K-token output that makes the
> grouping call slow, so a fix built purely around a longer timeout risked being obsoleted by it. CLO-798
> shipped visibility and left `DEFAULT_TIMEOUT_SECS`, the retry constants and `is_retryable` untouched,
> so CLO-801 can still land without undoing it - and a 515-file grouping call still needs
> `GCM_HTTP_TIMEOUT_SECS` until one of the two input-side fixes lands.
>
> **CLO-799 did not close the complaint, and that is a dependency worth naming (2026-09-17).** CLO-799
> shipped in v0.8.0 and fixed the step it targeted - the enable-models multiselect. The user hit the
> same symptom again, because `gcm provider` decides the default in a second prompt that CLO-799 never
> touched. CLO-802 fixes that prompt and depends on CLO-799 only in the sense that it works on the
> surface CLO-799 left behind; the two do not conflict. CLO-800 also lives in this wizard
> (`merge_provider_config` resets `[conflict]`) and is untouched by CLO-802, so it can land in either
> order.
>
> **Phase 6 (library extraction) is now complete.** CLO-598 merged in PR #52 (2026-07-30) — the out-of-tree `smoke/` consumer and `scripts/check-public-surface.sh` lock the public surface. No remaining Phase-6 blockers. Two owner-run HITL checks remain outstanding on issues already marked Done: **CLO-537**'s live ADC end-to-end check (needs the GCP project + `gcloud auth application-default login`) and **CLO-545**'s AC7 live OpenAI smokes (needs `OPENAI_API_KEY`). **CLO-554** (rebase resolve-until-clean loop) merged in PR #47 (2026-07-28), built on the **CLO-555** transaction engine from PR #35 — **Phase 4 (`gcm resolve`) is complete** and it blocked nothing downstream. **CLO-545** (OpenAI GPT-5.6 model refresh) merged in PR #34 (2026-07-11); the owner's live API smokes (AC7, need `OPENAI_API_KEY`) are the only remaining step. **CLO-547** (provider-wide model-discovery hardening, split from the CLO-545 review) merged in PR #38 (2026-07-22). **CLO-537** (Vertex AI provider, keyless ADC) merged in PR #32 (2026-07-09) — the only remaining step is the maintainer's live ADC end-to-end check (**HITL**). All prior tracked gcm work is Done; CLO-533 (`gcm resolve` remote MR/PR orchestration, Phase 2) merged in PR #30.

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
