# Project Dashboard - gcm

**Last Updated**: 2026-06-22 (CLO-496 onboarding wizard implemented → In Progress (PR pending); CLO-488 finalized to Done in Linear + docs — PR #6 merged 2026-06-21, `9052a7e`; CLO-490 optional secret scanning + gcmignore merged PR #16, Done; CLO-494 Anthropic provider merged PR #11, Done; CLO-495 Ollama local provider merged PR #14, Done; CLO-493 automation surface merged PR #12, Done; CLO-489 provider trait merged PR #10; CLO-492 PR #9; CLO-491 merged on main)

> `gcm` is a Rust CLI that turns working-tree changes into clean, logically-grouped,
> GPG-signed git commits. An LLM splits the diff into semantic groups and commits one
> group per run. This is a ground-up rewrite of the v2.7 bash tool
> (`docs/tmp/git-commit-ai.sh`), talking to provider HTTP APIs directly. PRD:
> [prds/prd-gcm.md](prds/prd-gcm.md).

## Sync

- **Source of truth**: Linear team **Cloud-ai** (`CLO`). Issues [CLO-485 … CLO-497](https://linear.app/cloud-ai/team/CLO).
- **Synced**: 2026-06-20. When an issue's status/label/blocker changes in Linear, mirror it here (and vice versa).
- **Labels**: `AFK` = an agent can implement and merge without human input. `HITL` = needs a human decision/credentials/review.
- **Priority**: High = Must-level / blocking. Medium = Should. Low = optional.

## All Tasks (master table)

| ID | Slice | Title | Label | Priority | Status | Blocked by | Covers (FR) |
|----|-------|-------|-------|----------|--------|------------|-------------|
| [CLO-485](https://linear.app/cloud-ai/issue/CLO-485) | S0 | Lock foundational architecture decisions + verify provider capabilities (ADR) | HITL | High | Done | — | 52; unblocks 10/27/40/45/54 |
| [CLO-486](https://linear.app/cloud-ai/issue/CLO-486) | S1 | Single-commit tracer: AI message via Groq with safe diff read | AFK | High | Done | CLO-485 | 4,5,6,9,10,18a,31a,32,34,35,36,39,41,47,48,49,57 |
| [CLO-487](https://linear.app/cloud-ai/issue/CLO-487) | S2 | Semantic grouping → commit first group | AFK | High | Done | CLO-486 | 1,2a,3,7,15,16,19,23a,24a,31,33 |
| [CLO-488](https://linear.app/cloud-ai/issue/CLO-488) | S4 | Resilient provider calls: typed errors + retries | AFK | High | Done | CLO-486 | 20,21,22 |
| [CLO-489](https://linear.app/cloud-ai/issue/CLO-489) | S6 | Provider trait + Gemini + OpenAI backends | AFK | High | Done | CLO-486, CLO-485 | 11,12,13a,14,17,18b,52 |
| [CLO-490](https://linear.app/cloud-ai/issue/CLO-490) | S10 | Optional secret scanning + `gcmignore` | AFK | Low | Done | CLO-486 | 50 |
| [CLO-491](https://linear.app/cloud-ai/issue/CLO-491) | S3 | Per-repo plan cache with commit-safe advancement | AFK | High | Done | CLO-487, CLO-485 | 2,8,25,26,27,28,29,30,45,58 |
| [CLO-492](https://linear.app/cloud-ai/issue/CLO-492) | S5 | Full plan validation + safe fallback | AFK | High | Done | CLO-487, CLO-488 | 23,24,46,47 |
| [CLO-493](https://linear.app/cloud-ai/issue/CLO-493) | S9 | Automation surface: `--json`, `--yes`/`--plan-only`, logging | AFK | Medium | Done | CLO-487 | 37,38,51 |
| [CLO-494](https://linear.app/cloud-ai/issue/CLO-494) | S7 | Anthropic provider via forced tool-use | AFK | Medium | Done | CLO-489, CLO-485 | 13b,18c |
| [CLO-495](https://linear.app/cloud-ai/issue/CLO-495) | S8 | Ollama local provider (zero-egress) | AFK | Medium | Done | CLO-489 | 56 |
| [CLO-496](https://linear.app/cloud-ai/issue/CLO-496) | S11 | First-run onboarding wizard | HITL | High | In Progress | CLO-485, CLO-489 | 40,53,54,55 |
| [CLO-497](https://linear.app/cloud-ai/issue/CLO-497) | S12 | Cross-platform releases + alias cutover | AFK | Medium | Backlog | CLO-487…CLO-496 | 42,43,44 |

All 58 functional requirements are allocated; `a`/`b`/`c` mark partial → full progressions across slices.

## Dependency tree

```
CLO-485  S0  ADR / decisions (HITL)            ← start here, gates everything
├─ CLO-486  S1  single-commit tracer
│  ├─ CLO-487  S2  grouping
│  │  ├─ CLO-491  S3  plan cache            (+CLO-485 message contract)
│  │  ├─ CLO-492  S5  validation+fallback   (+CLO-488)
│  │  ├─ CLO-493  S9  automation flags
│  │  └─ CLO-497  S12 release+cutover       (+ all feature slices)
│  ├─ CLO-488  S4  errors+retry  ───────────→ CLO-492
│  ├─ CLO-489  S6  provider trait+Gemini+OpenAI  (+CLO-485)
│  │  ├─ CLO-494  S7  Anthropic             (+CLO-485 auth)
│  │  ├─ CLO-495  S8  Ollama (local)
│  │  └─ CLO-496  S11 onboarding (HITL)     (+CLO-485 config/default)
│  └─ CLO-490  S10 secret scan (optional)
```

**Critical path:** CLO-485 → CLO-486 → CLO-487 → CLO-491/CLO-492 → … → CLO-497.

**Two parallel fronts after the tracer (CLO-486):** the workflow chain (CLO-487 → CLO-491 → CLO-492) and the provider chain (CLO-489 → CLO-494/CLO-495).

## Active Work (WIP Limit: 3)

| Task | Title | Status | Phase | Blocked By |
|------|-------|--------|-------|------------|
| [CLO-496](https://linear.app/cloud-ai/issue/CLO-496) | Add first-run onboarding wizard for provider setup | In Progress | PR | - |

## Up Next (Ready - no open blockers)

| Priority | Task | Title | Dependencies | Target |
|----------|------|-------|--------------|--------|
| — | — | None ready — CLO-496 is the last open feature slice (in progress); CLO-497 follows it | — | — |

> **CLO-496** (first-run onboarding wizard) started 2026-06-22 — In Progress, the last open feature slice. All other feature work is Done: **CLO-488** (typed errors + retries) merged PR #6 (`9052a7e`) and finalized to Done; **CLO-490** (secret scanning + `gcmignore`) PR #16; **CLO-494** (Anthropic, forced tool-use) PR #11; **CLO-495** (Ollama, zero-egress) PR #14; CLO-491 (plan cache, PR #7), **CLO-492** (validation, PR #9), **CLO-493** (automation surface, PR #12), **CLO-489** (provider trait, PR #10). CLO-497 (release + cutover) now waits only on CLO-496.

## Blocked

| Task | Title | Blocked By | Notes |
|------|-------|------------|-------|
| CLO-497 | Release + cutover | CLO-496 | Ships after the v1 feature set (CLO-487/488/489/490/491/492/493/494/495 done; waits only on CLO-496) |

## Recently Completed

| Task | Title | Completed | Summary |
|------|-------|-----------|---------|
| CLO-488 | Resilient provider calls: typed errors + retries | 2026-06-22 | FR-20/21/22. Typed provider-error taxonomy (rate-limit/bad-request/server/timeout/auth/parse) with bounded exponential backoff on 429/5xx, never on 400/auth; defensive parsing fallback when structured output is unavailable; distinct actionable message per error type, error kind visible in debug logs. PR #6 (`9052a7e`) merged 2026-06-21; its retry engine was later moved into shared `http.rs` by CLO-489. Linear/aggregation finalize had lagged the merge (issue sat Backlog) — reconciled to Done 2026-06-22. Unblocks CLO-497 (now waits only on CLO-496). |
| CLO-490 | Optional secret scanning + `gcmignore` | 2026-06-22 | Opt-in privacy layer before provider egress (FR-50). New `src/privacy.rs` parses `.gcmignore`/`gcmignore` + glob-matches paths, filtering changed files before cache/validation/display/staging; provider-bound grouping/single diffs rebuilt from the filtered path set so ignored tracked files can't leak via whole-tree `git diff` (rename/copy excluded if either path matches). Pre-send secret scan via `--secret-scan`/`GCM_SECRET_SCAN` (`off`/`redact`/`abort`): redact strips credential spans, abort exits before any request; ignore files excluded from prompts by default. Also de-raced a pre-existing flaky CLO-494 env-var test (merged two racing `GCM_ANTHROPIC_BASE_URL` tests). 161 unit + 237 acceptance (0 FAIL); CI green (ubuntu+macos). PR #16 (squash) merged. Unblocks CLO-497 (now waits only on CLO-496). |
| CLO-494 | Anthropic provider via forced tool-use | 2026-06-22 | Anthropic backend behind the CLO-489 `Provider` trait (FR-13b/18c): direct Messages API (`/v1/messages`) with forced tool-use (`tool_choice:{type:tool,name:commit_plan}`, `input_schema` from `plan::schema()`) for the typed grouping plan. `x-api-key` + `anthropic-version: 2023-06-01` via the new `HttpRequest.extra_headers` Vec; adaptive-thinking content blocks skipped + `<think>` backstop (no CoT into plan/message); `max_tokens` stop_reason guard; default `claude-haiku-4-5`. 15 unit tests (156 total); Codex + Gemini pre-PR validation PASS_WITH_NOTES. Merged origin/main (CLO-495 Ollama + CLO-493 automation) at the PR checkpoint, reconciling `HttpRequest.auth`→`Option` (key-free Ollama) with the new `extra_headers` field. PR #11 merged. |
| CLO-495 | Ollama local provider (zero-egress) | 2026-06-22 | Local, key-free Ollama backend behind the CLO-489 `Provider` trait (FR-56): native `/api/chat` with a JSON-Schema `format` (modeled on `gemini.rs`), reads `message.content` / ignores `message.thinking`, `stream:false`. `HttpRequest.auth`→`Option` (key-free, no `Authorization` header); `classify_status` no-auth 401/403→`Http`. Actionable errors: unreachable→`Transport` (`ollama serve`/`OLLAMA_HOST`), 404→`Config` (`ollama pull`). `OLLAMA_HOST` scheme/port normalization; default `gemma4:e4b-mlx`; `:cloud`-egress warning. 139 unit + 230 acceptance + real-daemon e2e (gemma4:12b); Gemini+Ollama spec review APPROVE; Codex FAIL→fixed (no-auth env-var leak)→converged. Merged origin/main (CLO-493) at the PR checkpoint. PR #14 (squash) merged. |
| CLO-489 | Provider trait + Gemini + OpenAI backends | 2026-06-21 | Synchronous `Provider` trait + flag/env registry (`src/provider/`); Groq refactored onto it; Gemini (`generateContent`/`responseSchema`/`thinkingLevel`) + OpenAI (strict `json_schema`, o-series payload path) backends. `GroqError`→provider-agnostic `ProviderError{provider,kind}`; CLO-488 retry engine moved to shared `http.rs`. Selection flag>env>default groq; per-provider model env + diff budgets; cache fingerprint folds provider+model (key unchanged, FR-25); per-model reasoning suppression + `<think>` backstop (no CoT). Behavioral parity for bare `gcm`. 105 unit + 161 acceptance; Gemini PASS + Codex FAIL→fixed→PASS_WITH_NOTES; Copilot no comments. Spec workflow (round-2 user review: 6 pts). Merged origin/main (CLO-492) twice at the PR checkpoint. PR #10 (squash) merged. |
| CLO-492 | Full plan validation + safe fallback | 2026-06-21 | FR-23 full bijective validation (`plan::validate`): rejects omissions, cross-group duplicates, empty groups (new `PlanError::{EmptyGroup,DuplicateFile,OmittedFile}`) - the bash validator only caught unknown files. FR-46 runtime curated-index warning (`is_staged`/`is_partially_staged` + `ui::curated_index_warning`) before any index reset, even under `--yes`, silent on `--dry-run`. Cache-hit re-validation (`validate_cached`, partition-only) so a pre-CLO-492 cache can't replay an omission. FR-24/FR-47 verified (fallback already post-retry + post-confirm staging + index restore from CLO-488/491). 101 unit + 167 acceptance; Gemini PASS + Codex FAIL→fixed→PASS (caught a cache-hit bypass); Copilot 1 fixed + 1 pushed back. Spec workflow. PR #9 (squash) merged. |
| CLO-491 | Per-repo plan cache with commit-safe advancement | 2026-06-21 | Per-repo plan cache (`src/cache.rs`): `sha256(repo-root)` key in the OS cache dir, `0600`; streamed content fingerprint (no HEAD pin, unborn-safe) so re-runs commit the next group with no grouping call; regenerate-per-group message on hit; `CommitFailed`/`CommitOutcome` gate leaves the group staged + un-advanced on a rejected hook (FR-58); `--reset`/`--all`/fallback clear. Fixed the bash name-only-staleness + null-message-advancement bugs. 58 unit + 117 acceptance; Gemini PASS + Codex FAIL→fixed→PASS; Copilot 2 comments addressed. Dev workflow (discovery→design→plan→implement). PR #7 (squash) merged. |
| CLO-487 | Semantic grouping → commit first group | 2026-06-20 | Structured-output grouping plan (typed Plan/Group, strict json_schema) → commit group 1; re-run advances. `-uall` NUL status parse, literal NUL-stdin staging (rename-safe, glob-safe, ARG_MAX-safe), per-file diff truncation, merge-conflict abort, announced single-commit fallback. 39 unit + 73 acceptance tests; Gemini PASS + Codex FAIL→fixed→PASS. PR #5 (squash) merged. Unblocked CLO-491/493. |
| CLO-486 | Single-commit tracer: AI message via Groq with safe diff read | 2026-06-19 | Rust scaffold + tracer: safe diff read → Groq message → `[Y/n/e]` → signed commit. 15 unit + 35 acceptance tests; 3 Codex validation passes. PR #4 (squash) merged. Unblocked CLO-487/488/489/490. |
| CLO-485 | Foundational architecture decisions + capability matrix (ADR) | 2026-06-19 | ADR-001 (Accepted): 13 decisions locked + 6-provider capability matrix verified. Cerebras dropped; default→Groq. PR #2 merged. |
