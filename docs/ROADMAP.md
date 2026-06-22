# Roadmap

**Last Updated**: 2026-06-22

## Phase 1: Foundation

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-485](https://linear.app/cloud-ai/issue/CLO-485/lock-foundational-architecture-decisions-and-verify-provider) | Lock foundational architecture decisions and verify provider capabilities (ADR) | Done | - |
| [CLO-486](https://linear.app/cloud-ai/issue/CLO-486/add-single-commit-tracer-ai-message-via-groq-with-safe-diff-read) | Add single-commit tracer AI message via Groq with safe diff read | Done | CLO-485 |
| [CLO-489](https://linear.app/cloud-ai/issue/CLO-489/add-provider-trait-with-gemini-and-openai-backends) | Add provider trait with Gemini and OpenAI backends | Done | CLO-485, CLO-486 |

## Phase 2: Provider Expansion

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-494](https://linear.app/cloud-ai/issue/CLO-494/add-anthropic-provider-via-forced-tool-use) | Add Anthropic provider via forced tool-use | In Progress | CLO-485, CLO-489 |

## Phase 3: Release

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-497](https://linear.app/cloud-ai/issue/CLO-497/ship-cross-platform-releases-and-the-alias-cutover) | Ship cross-platform releases and the alias cutover | Backlog | CLO-494 |

## Summary

| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 1: Foundation | 3 | 3 | Complete |
| Phase 2: Provider Expansion | 1 | 0 | In Progress |
| Phase 3: Release | 1 | 0 | Not Started |