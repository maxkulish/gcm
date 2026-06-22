# PRD: CLO-494 — Add Anthropic provider via forced tool-use

**Linear:** [CLO-494](https://linear.app/cloud-ai/issue/CLO-494/add-anthropic-provider-via-forced-tool-use)
**ADR:** [ADR-001](../adrs/001-foundational-architecture-decisions.md) Decisions 2, 3
**Parent PRD:** [prd-gcm.md](prd-gcm.md) FR-13, FR-16, FR-17, FR-18

## Goal

Add Anthropic as a fourth provider behind the existing `Provider` trait (CLO-489),
using the direct Messages API (ADR-001 Decision 3 — no `claude` CLI). Structured
output is achieved via forced tool-use (`tools` + `tool_choice:{type:"tool"}` +
`input_schema`), since Anthropic has no generic `response_format`.

## Scope

- `Anthropic` struct implementing `Provider` in `src/provider/anthropic.rs`
- Forced tool-use for the grouping plan (`generate_plan`)
- Plain Messages API call for per-group message regeneration (`generate_message`)
- `ANTHROPIC_API_KEY` env-var resolution (FR-18)
- `anthropic-version` header (requires extending `HttpRequest` with an extra header)
- `ProviderId::Anthropic` variant + `--provider=anthropic` / `GCM_PROVIDER=anthropic`
- `GCM_ANTHROPIC_MODEL` env var + a sensible default model
- `GCM_ANTHROPIC_BASE_URL` env var override (for proxy/testing)

## Out of scope

- Onboarding wizard integration (separate task)
- `output_config.format` JSON outputs (newer API, only on Opus 4.8 / Sonnet 4.6 /
  Haiku 4.5 — tool-use is universally available and the issue title specifies it)
- Streaming (ADR-001 Decision 2: blocking HTTP)

## Acceptance Criteria

1. `gcm --provider=anthropic` produces a grouped commit with a valid typed plan
2. Reasoning/thinking output never reaches the commit message
3. Missing `ANTHROPIC_API_KEY` produces a clear, actionable error naming the env var
4. `GCM_ANTHROPIC_MODEL` overrides the default model; `--model` overrides both

## Context

- Extends [CLO-489](https://linear.app/cloud-ai/issue/CLO-489) provider trait
- ADR-001 Decision 3: direct Messages API only, `ANTHROPIC_API_KEY`, no `claude` CLI
- ADR-001 capability matrix: Anthropic has no generic `response_format`; structured
  output is a forced tool call (`tools` + `tool_choice` + `input_schema`)
- Reasoning suppression: adaptive thinking (`thinking:{type:"adaptive"}`); CoT
  `display` omitted by default — the universal `dimd` strip is the backstop
- The primary user (Max) currently aliases `gcm` → Anthropic Haiku via subscription
  `claude` CLI; the rewrite uses the direct API (paid key required, documented)