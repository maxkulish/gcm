# Roadmap - gcm

**Last Updated**: 2026-07-08 (synced from Linear; added CLO-537 Vertex provider)

## Summary

| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 1: Foundations | 13 | 13 | Complete |
| Phase 2: Hardening | 1 | 1 | Complete |
| Phase 3: v2 Introspection & Config | 2 | 2 | Complete |
| Phase 4: `gcm resolve` (conflict resolution) | 2 | 2 | Complete |
| Phase 5: Provider expansion | 1 | 0 | Not Started |
| Bug fixes (cross-cutting) | 3 | 3 | Complete |

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
| CLO-497 | Cross-platform releases + alias cutover | Done | CLO-487…CLO-496 |

## Phase 2: Hardening

Source: [PRD: gcm](prds/prd-gcm.md) FR-60 (added 2026-06-23, `e89ee14`). Replaces the best-effort secret matcher shipped in CLO-490 (FR-50).

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| CLO-514 | Replace best-effort secret scanner with rule-pack + entropy engine | Done | CLO-490 (related) |

## Phase 3: v2 Introspection & Config

Post-migration usability slices: read-only introspection and interactive provider/model configuration.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| CLO-515 | `gcm status` (active providers, models, paths, config sources) | Done | CLO-493, CLO-485, CLO-496 (related) |
| CLO-516 | Interactive `gcm provider` picker (cliclack, Goose-style) | Done | none |

## Phase 4: `gcm resolve` (conflict resolution)

New feature area: LLM-assisted git merge/rebase/cherry-pick conflict resolution, built on the existing `Provider` layer. Discovery: layered pipeline (`zdiff3` → optional `mergiraf` → provider → validation gate → preview), LLM as last resort.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| CLO-531 | `gcm resolve` LLM-assisted resolver (Phase 1: local markers) | Done | CLO-489, CLO-487, CLO-496/516, CLO-490/514 (all Done, related) |
| CLO-533 | `gcm resolve` remote MR/PR conflict orchestration (Phase 2) | Done | CLO-531 |

## Phase 5: Provider expansion

New backend: Google **Vertex AI** as a first-class provider (`ProviderId::Vertex`) with keyless ADC auth, selectable in `gcm provider` alongside the AI Studio `Google` backend. Thin wrapper over the existing `gemini.rs` payloads (only URL + auth differ). Design doc (**Draft**): [designs/clo-537-vertex-provider.md](designs/clo-537-vertex-provider.md).

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| CLO-537 | Add Vertex AI provider (keyless ADC) selectable in `gcm provider` | Backlog | CLO-489, CLO-516, CLO-531 (all Done) |

## Bug fixes (cross-cutting)

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| CLO-517 | Fix Ollama cloud model commit-plan parse failure (single-commit fallback) | Done | CLO-495 (related) |
| CLO-534 | Fix `gcm resolve` HTTP 400 on Gemini (unsupported `additionalProperties`) | Done | CLO-531 |
| CLO-535 | Fix `gcm resolve` splice: missing trailing newline joins the following line | Done | CLO-531 (related) |
