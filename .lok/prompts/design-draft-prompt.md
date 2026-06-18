You are drafting a design document for the **gcm** Rust project.

## Inputs

- Task ID: __TASK_ID__
- Slug: __SLUG__
- Title: __TITLE__
- Workflow state: docs/status/__TASK_LC__-workflow.yaml (read history events `codebase_analyzed`, `discovery_approved`, and any `approach_chosen` in `phases.discovery`)
- Discovery report (optional): docs/discovery/__TASK_LC__.md - read if present, otherwise rely on the workflow YAML history
- PRDs: docs/prd/*.md (read whichever the task title references)
- Conventions: AI-AGENTS.md, README.md, src/CLAUDE.md (if present)

## Project context

gcm is a Rust CLI project. [TODO: refine gcm project description]

Inspect the repository (`src/`, `Cargo.toml`, conventions docs) to determine the
actual crate layout, binaries, and module structure before drafting.

Pre-merge gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.

## Required output

Write a single markdown file to `docs/designs/__TASK_LC__-__SLUG__.md` with EXACTLY these 8 sections in order:

1. **# Design: __TASK_ID__ - __TITLE__** (top-level heading, first line)
2. **## Problem** - 1 paragraph citing the discovery context. WHO is affected, WHAT is broken or missing, WHY it matters now.
3. **## Goals / Non-goals** - bulleted lists. Goals are concrete deliverables; non-goals are explicit exclusions.
4. **## Architecture** - modules, data flow, concrete Rust types. ASCII diagram if it helps. Name the source path (e.g. `src/backend/context.rs`) every new module / type lands in.
5. **## Public API surface** - Rust trait and struct signatures exactly as they will appear. Use real Rust syntax in code fences. Show before/after for any signature change.
6. **## Assumptions** - one bullet per assumption with confidence (high|medium|low) and verification path. An empty list is acceptable but the section heading is mandatory.
7. **## Test plan** - unit tests (named functions), integration tests under `tests/`, manual verification steps. For backend trait work, include a per-backend test matrix.
8. **## Migration / rollout** - backward compatibility notes, feature flags if needed, rollout order. If the change is purely additive say so explicitly.
9. **## Open questions** - unresolved decisions from discovery. Leave them genuinely open with tradeoff description, not fabricated answers.

(The 8-section count is a deliberate semantic grouping; the design heading itself is section 1 in the layout above.)

## Hard rules

- Start directly with `# Design: __TASK_ID__ - __TITLE__`. No preamble. No "I will now design", "Based on the discovery", "Here is my draft", or any sentence describing what you are about to do.
- No chain-of-thought, scratchpad, internal monologue, or `<think>` blocks. Output only the final design markdown.
- Leave open questions genuinely open. Do not fabricate resolutions.
- Do not invent implementation details not supported by the discovery outputs or the PRD.
- Never recommend abandoning the discovery-phase `approach_chosen` without flagging it as an open question.
- Do not propose dependency additions unless strictly required. Prefer gcm's existing dependency set.
- Synthesize the discovery context; never paste workflow YAML history back at the reader.
- If you need to show template or variable syntax in the design doc, use angle brackets (`<ARG_1>`, `<STEP_OUTPUT>`) - never literal `{{` or `}}`.
- Never reference vault content, PII, API keys, or Linear ticket bodies; cite paths and ticket IDs only.

Write the file directly via the Write tool, then print `WROTE: docs/designs/__TASK_LC__-__SLUG__.md` and stop.
