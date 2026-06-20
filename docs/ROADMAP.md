# Roadmap - gcm

**Last Updated**: 2026-06-20 (CLO-487 grouping started)

## Summary

| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 1: Foundations | 13 | 2 | In Progress |

## Phase 1: Foundations

Source: [PRD: gcm](prds/prd-gcm.md) §8 Open Questions; foundational decisions in [ADR-001](adrs/001-foundational-architecture-decisions.md).

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| CLO-485 | Foundational architecture decisions + capability matrix (ADR) | Done | none |
| CLO-486 | Single-commit tracer | Done | CLO-485 |
| CLO-487 | Semantic grouping → commit first group | In Progress | CLO-486 |
| CLO-488 | Resilient provider calls: typed errors + retries | Ready | CLO-486 |
| CLO-489 | Provider trait + Gemini + OpenAI backends | Ready | CLO-486, CLO-485 |
| CLO-490 | Optional secret scanning + `gcmignore` | Ready | CLO-486 |
