# Roadmap - gcm

**Last Updated**: 2026-06-23 (CLO-514 secret-scanner rule-pack + entropy engine merged PR #18, Done — new FR-60, hardens FR-50. CLO-497 release/cutover still Ready, the only open slice. CLO-496 first-run onboarding wizard merged PR #17, Done; CLO-490 secret scanning + gcmignore merged PR #16, Done; CLO-488 finalized to Done — PR #6 merged 2026-06-21, `9052a7e`; CLO-494 Anthropic provider merged PR #11, Done; CLO-495 Ollama local provider merged PR #14, Done; CLO-493 automation surface merged PR #12, Done; CLO-489 merged PR #10; CLO-492 merged PR #9; CLO-491 merged on main)

## Summary

| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 1: Foundations | 13 | 13 | Complete |
| Phase 2: Hardening | 1 | 1 | Complete |

## Phase 1: Foundations

Source: [PRD: gcm](prds/prd-gcm.md) §8 Open Questions; foundational decisions in [ADR-001](adrs/001-foundational-architecture-decisions.md).

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| CLO-485 | Foundational architecture decisions + capability matrix (ADR) | Done | none |
| CLO-486 | Single-commit tracer | Done | CLO-485 |
| CLO-487 | Semantic grouping → commit first group | Done | CLO-486 |
| CLO-488 | Resilient provider calls: typed errors + retries | Done | CLO-486 |
| CLO-489 | Provider trait + Gemini + OpenAI backends | Done | CLO-486, CLO-485 |
| CLO-490 | Optional secret scanning + `gcmignore` | Done | CLO-486 |
| CLO-491 | Per-repo plan cache with commit-safe advancement | Done | CLO-487, CLO-485 |
| CLO-492 | Full plan validation + safe fallback | Done | CLO-487, CLO-488 |
| CLO-493 | Automation surface: `--json`, `--yes`/`--plan-only`, logging | Done | CLO-487 |
| CLO-494 | Anthropic provider via forced tool-use | Done | CLO-489, CLO-485 |
| CLO-495 | Ollama local provider (zero-egress) | Done | CLO-489 |
| CLO-496 | First-run onboarding wizard | Done | CLO-485, CLO-489 |
| CLO-497 | Cross-platform releases + alias cutover | Ready | CLO-487…CLO-496 |

## Phase 2: Hardening

Source: [PRD: gcm](prds/prd-gcm.md) FR-60 (added 2026-06-23, `e89ee14`). Replaces the best-effort secret matcher shipped in CLO-490 (FR-50).

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| CLO-514 | Replace best-effort secret scanner with rule-pack + entropy engine | Done | CLO-490 (related) |
