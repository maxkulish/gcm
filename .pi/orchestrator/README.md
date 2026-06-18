# gcm pi Orchestrator

Phase scripts that drive the gcm task lifecycle through pi. Each file
in `phases/` is loaded on demand by
`.pi/extensions/orchestrate/index.ts` when the orchestrator dispatches a
phase.

The single source of truth for any task is its workflow YAML at
`docs/status/clo-XX-workflow.yaml`. Phase scripts only mutate state via
the `update_workflow_state` and `transition_phase` tools registered by
the extension - never by editing the YAML directly.

## Schema parity with Claude

These phase scripts mirror `.claude/commands/task/phases/*.md`. A task
started in Claude can be resumed in pi (and vice-versa) because both
sides:

- Use the same YAML schema (top-level keys, phase blocks, history shape).
- Use the same allowed transitions (see `phases/*.md` and `index.ts`).
- Use the same required-fields / required-history checks
  (`PHASE_CONFIG` in `index.ts`).
- Reach the same Linear workspace (`Cloud-ai`, identifier `CLO`). The
  Claude side uses the `mcp__linear-server__*` prefix from the global
  MCP config; the pi side serves the approved 7-tool subset through
  the sibling bridge at `.pi/extensions/linear/` under the
  `mcp__linear__` prefix. Phase scripts reference Linear actions
  abstractly (e.g. "get the issue", "comment on the ticket") and call
  the runtime's available prefix.

When you change a phase script, mirror the change on the Claude side
(`.claude/commands/task/phases/<phase>.md`). Otherwise the two sides
will drift and resume will break.

## Phase index

| Phase | File | Purpose |
|---|---|---|
| init | `phases/init.md` | Parse args, classify task, create / resume YAML |
| discovery | `phases/discovery.md` | Frame problem, choose approach |
| design | `phases/design.md` | Design doc + AI design review |
| plan | `phases/plan.md` | Decompose into testable sub-tasks |
| spec | `phases/spec.md` | Specification path (skips discovery / design / plan) |
| operational | `phases/operational.md` | Operational task path (audits, migrations) |
| implement | `phases/implement.md` | Land sub-tasks + codex+gemini validation gate |
| pr | `phases/pr.md` | Pre-flight checks, open PR, address reviews |
| complete | `phases/complete.md` | Merge, sync project files, finalize |
| status | `phases/status.md` | Read-only display |
| blocked | `phases/blocked.md` | Pseudo-phase: workflow.status = blocked |

There is intentionally **no `review` phase**. The codex+gemini
validation gate runs inside `implement.md` step 5, before transitioning
to `pr`.

## Allowed transitions (mirror of `index.ts`)

```
init        -> discovery | spec | operational
discovery   -> design
design      -> plan
plan        -> implement
spec        -> implement
implement   -> pr
pr          -> complete
operational -> execute | document | complete
execute     -> document | complete
document    -> complete | pr
complete    -> (terminal)
```

## Conventions enforced by every phase script

- Task IDs are `CLO-NN` (any case). Status files: `clo-NN-workflow.yaml`.
- Branches: `feat/clo-NN-<short-slug>`.
- Pre-merge gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`. PR phase enforces it explicitly.
- Validation gate: `lok workflow run pre-pr-validation <design_doc> <plan_file> CLO-XX` (see `.lok/workflows/pre-pr-validation.toml`). `implement.md` step 4 runs it.
- Aggregation files: `PROJECT.md`, `ROADMAP.md`, `DEPENDENCIES.md`. `init` calls `/project:sync CLO-XX --start`; `complete` calls `/project:sync CLO-XX --complete`. Phase scripts must NEVER hand-edit these files.
- Linear status flow: `Backlog -> Todo -> In Progress -> In Review -> Done`.
- Comment on Linear at every phase transition (see Linear MCP guide).

## Adding a phase

1. Add it to `ALLOWED_TRANSITIONS` in `index.ts`.
2. Add it to `PHASE_CONFIG` (required fields and history events).
3. Add it to `TYPE_ALLOWED_PHASES` for the relevant task type(s).
4. Write `phases/<new>.md` following the existing structure: required
   exit state, numbered steps, explicit `update_workflow_state` and
   `transition_phase` calls.
5. Mirror all of the above in the Claude flow under
   `.claude/commands/task/`.

## See also

- `../extensions/orchestrate/README.md` - extension-level docs
- `AI-AGENTS.md` (repo root) - gcm project overview
- `.claude/commands/task/orchestrate.md` - Claude-side flow
- `.lok/workflows/pre-pr-validation.toml` - validation gate runner
- `PROJECT.md`, `ROADMAP.md`, `DEPENDENCIES.md` - aggregation files synced by `/project:sync`
