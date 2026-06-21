# Project Dashboard - gcm

**Last Updated**: 2026-06-21 (CLO-489 provider trait + Gemini/OpenAI started, Spec phase; CLO-488 PR #6; CLO-491 merged on main)

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
| [CLO-485](https://linear.app/cloud-ai/issue/CLO-485) | S0 | Lock foundational architecture decisions + verify provider capabilities (ADR) | HITL | High | Backlog | — | 52; unblocks 10/27/40/45/54 |
| [CLO-486](https://linear.app/cloud-ai/issue/CLO-486) | S1 | Single-commit tracer: AI message via Groq with safe diff read | AFK | High | Done | CLO-485 | 4,5,6,9,10,18a,31a,32,34,35,36,39,41,47,48,49,57 |
| [CLO-487](https://linear.app/cloud-ai/issue/CLO-487) | S2 | Semantic grouping → commit first group | AFK | High | Done | CLO-486 | 1,2a,3,7,15,16,19,23a,24a,31,33 |
| [CLO-488](https://linear.app/cloud-ai/issue/CLO-488) | S4 | Resilient provider calls: typed errors + retries | AFK | High | Backlog | CLO-486 | 20,21,22 |
| [CLO-489](https://linear.app/cloud-ai/issue/CLO-489) | S6 | Provider trait + Gemini + OpenAI backends | AFK | High | In Progress | CLO-486, CLO-485 | 11,12,13a,14,17,18b,52 |
| [CLO-490](https://linear.app/cloud-ai/issue/CLO-490) | S10 | Optional secret scanning + `gcmignore` | AFK | Low | Backlog | CLO-486 | 50 |
| [CLO-491](https://linear.app/cloud-ai/issue/CLO-491) | S3 | Per-repo plan cache with commit-safe advancement | AFK | High | Done | CLO-487, CLO-485 | 2,8,25,26,27,28,29,30,45,58 |
| [CLO-492](https://linear.app/cloud-ai/issue/CLO-492) | S5 | Full plan validation + safe fallback | AFK | High | Backlog | CLO-487, CLO-488 | 23,24,46,47 |
| [CLO-493](https://linear.app/cloud-ai/issue/CLO-493) | S9 | Automation surface: `--json`, `--yes`/`--plan-only`, logging | AFK | Medium | Backlog | CLO-487 | 37,38,51 |
| [CLO-494](https://linear.app/cloud-ai/issue/CLO-494) | S7 | Anthropic provider via forced tool-use | AFK | Medium | Backlog | CLO-489, CLO-485 | 13b,18c |
| [CLO-495](https://linear.app/cloud-ai/issue/CLO-495) | S8 | Ollama local provider (zero-egress) | AFK | Medium | Backlog | CLO-489 | 56 |
| [CLO-496](https://linear.app/cloud-ai/issue/CLO-496) | S11 | First-run onboarding wizard | HITL | High | Backlog | CLO-485, CLO-489 | 40,53,54,55 |
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
| [CLO-488](https://linear.app/cloud-ai/issue/CLO-488) | Resilient provider calls: typed errors + retries | In Progress | PR | - |
| [CLO-489](https://linear.app/cloud-ai/issue/CLO-489) | Provider trait + Gemini + OpenAI backends | In Progress | Spec | - |

## Up Next (Ready - no open blockers)

| Priority | Task | Title | Dependencies | Target |
|----------|------|-------|--------------|--------|
| Medium | CLO-493 | Automation surface: `--json`, `--yes`/`--plan-only`, logging | CLO-487 (done) | automation flags on the grouping path |
| Low | CLO-490 | Optional secret scanning + `gcmignore` | CLO-486 (done) | optional |

> CLO-491 (plan cache) merged (PR #7). **CLO-488** (typed errors + retries) is in progress — PR #6 open. **CLO-489** (provider trait + Gemini + OpenAI) started 2026-06-21 (Spec phase) — runs the provider chain in parallel and unblocks CLO-494/495/496. **CLO-490**, **CLO-493** remain ready. CLO-492 still waits on CLO-488; CLO-497 waits on the rest of the feature set.

## Blocked

| Task | Title | Blocked By | Notes |
|------|-------|------------|-------|
| CLO-492 | Validation + fallback | CLO-488 | CLO-487 done; still needs CLO-488 (errors+retry) |
| CLO-494 | Anthropic provider | CLO-489 | ADR-001 #3: direct Messages API, forced tool-use |
| CLO-495 | Ollama provider | CLO-489 | |
| CLO-496 | Onboarding wizard | CLO-489 | ADR-001 #4/#5/#11: config format + Groq default + GPG check |
| CLO-497 | Release + cutover | CLO-488…CLO-496 | Ships after the v1 feature set (CLO-487 done) |

## Recently Completed

| Task | Title | Completed | Summary |
|------|-------|-----------|---------|
| CLO-491 | Per-repo plan cache with commit-safe advancement | 2026-06-21 | Per-repo plan cache (`src/cache.rs`): `sha256(repo-root)` key in the OS cache dir, `0600`; streamed content fingerprint (no HEAD pin, unborn-safe) so re-runs commit the next group with no grouping call; regenerate-per-group message on hit; `CommitFailed`/`CommitOutcome` gate leaves the group staged + un-advanced on a rejected hook (FR-58); `--reset`/`--all`/fallback clear. Fixed the bash name-only-staleness + null-message-advancement bugs. 58 unit + 117 acceptance; Gemini PASS + Codex FAIL→fixed→PASS; Copilot 2 comments addressed. Dev workflow (discovery→design→plan→implement). PR #7 (squash) merged. |
| CLO-487 | Semantic grouping → commit first group | 2026-06-20 | Structured-output grouping plan (typed Plan/Group, strict json_schema) → commit group 1; re-run advances. `-uall` NUL status parse, literal NUL-stdin staging (rename-safe, glob-safe, ARG_MAX-safe), per-file diff truncation, merge-conflict abort, announced single-commit fallback. 39 unit + 73 acceptance tests; Gemini PASS + Codex FAIL→fixed→PASS. PR #5 (squash) merged. Unblocked CLO-491/493. |
| CLO-486 | Single-commit tracer: AI message via Groq with safe diff read | 2026-06-19 | Rust scaffold + tracer: safe diff read → Groq message → `[Y/n/e]` → signed commit. 15 unit + 35 acceptance tests; 3 Codex validation passes. PR #4 (squash) merged. Unblocked CLO-487/488/489/490. |
| CLO-485 | Foundational architecture decisions + capability matrix (ADR) | 2026-06-19 | ADR-001 (Accepted): 13 decisions locked + 6-provider capability matrix verified. Cerebras dropped; default→Groq. PR #2 merged. |
