# Pi Task Orchestrator Extension (gcm)

Pi extension that orchestrates the gcm task lifecycle (`CLO-XX`),
keeping schema parity with the Claude flow in
`.claude/commands/task/orchestrate.md` so a task started in Claude can be
resumed in Pi (and vice-versa).

## Architecture

1. **TypeScript state machine** (`index.ts`): `ALLOWED_TRANSITIONS`,
   `TYPE_ALLOWED_PHASES`, and `PHASE_CONFIG` enforce phase order, required
   fields, and required history events. The LLM cannot transition unless
   gates pass.
2. **Phase-isolated prompts** (`.pi/orchestrator/phases/*.md`): the
   orchestrator loads only the relevant phase file when dispatching.
3. **Linear via MCP**: a sibling extension at `.pi/extensions/linear/`
   connects to Linear's hosted MCP and re-exposes every Linear tool
   under the `mcp__linear__` prefix. Pi extensions don't inherit
   Claude's MCP servers, so this bridge is what makes phase scripts
   portable between Claude and pi without rewrites.
4. **Single source of truth**: `docs/status/clo-XX-workflow.yaml`. Format
   matches the Claude flow exactly.

## Differences vs the Mentis pi orchestrator

| Aspect | Mentis | gcm |
|---|---|---|
| Task ID | `MENTI-XX` | `CLO-XX` |
| Issue tracker | Plane.so | Linear (MCP) |
| Status file | `docs/status/menti-XX-workflow.yaml` | `docs/status/clo-XX-workflow.yaml` |
| Review phase | Separate `review` phase before `pr` | None - validation gate is inside `implement` |
| Stack | Tauri v2 + Rust + React | Rust CLI project |
| Pre-merge gate | `cargo fmt --manifest-path src-tauri/...` | `cargo fmt --check && cargo clippy -- -D warnings && cargo test` |

## Installation

Install both this orchestrator AND the sibling Linear bridge - the
phase scripts call `mcp__linear__*` tools, which only resolve when the
linear bridge is active.

```bash
cd .pi/extensions/orchestrate
npm install

cd ../linear
npm install
```

Enable:
```bash
# Temporary (load both)
pi -e .pi/extensions/orchestrate/index.ts -e .pi/extensions/linear/index.ts

# Permanent (symlink each)
ln -s $(pwd)/.pi/extensions/orchestrate ~/.pi/agent/extensions/gcm-task-orchestrator
ln -s $(pwd)/.pi/extensions/linear      ~/.pi/agent/extensions/gcm-linear

# Required env (linear bridge)
export LINEAR_API_KEY=lin_api_...
```

## Usage

```bash
/task:orchestrate CLO-330                   # start or resume
/task:orchestrate CLO-330 --status          # show current state only
/task:orchestrate CLO-330 --spec            # specification task
/task:orchestrate CLO-330 --ops             # operational task
/task:orchestrate CLO-330 --skip-discovery  # development, skip discovery
```

## Workflow YAML schema

Top-level keys that must remain stable across Claude and Pi:

```yaml
task_id: CLO-330
task_title: "..."
task_url: "https://linear.app/cloud-ai/issue/clo-330/..."
task_type: development | specification | operational
classification_reason: |
  ...

linear:
  team: Cloud-ai
  project: gcm
  status_at_start: Backlog
  priority: High
  branch_suggested: ...
  branch_actual: feat/clo-330-...
  blocks: []
  blocked_by: []

workflow:
  current_phase: implement
  status: active | blocked | paused | complete | in_progress | checkpoint
  created_at: ISO-8601
  updated_at: ISO-8601

phases:
  discovery: { status, approved, ... }
  spec: { status, spec_file, approved, review_completed, ... }
  design: { status, design_doc, draft_ready, finalized, review_completed, ... }
  plan: { status, plan_file, approved }
  implement: { status, commits[], codex_validated, codex_verdict, codex_report, gemini_validation_report }
  pr: { status, pr_url, pr_number, ci_passed, reviews_addressed, merged_at, merge_commit }
  complete: { status, aggregation_files_updated, merged_at }

history:
  - { timestamp, action, phase, details }
```

## Tools exposed to the LLM

### `update_workflow_state`

```ts
update_workflow_state({
  task_id: "CLO-330",
  phase: "implement",
  action: "implementation_complete",
  details: "All sub-tasks landed; cargo test green",
  phase_updates: { status: "complete", commits: ["abc123"] },
  workflow_updates: { current_phase: "pr", status: "in_progress" },
  linear_updates: { branch_actual: "feat/clo-330-foo" },
  root_updates: { task_title: "..." }
})
```

### `transition_phase`

```ts
transition_phase({
  task_id: "CLO-330",
  from_phase: "implement",
  to_phase: "pr"
})
```

Validation rules:
- `from_phase` must equal current `workflow.current_phase`
- `to_phase` must be in `ALLOWED_TRANSITIONS[from]`
- `to_phase` must be permitted for the task type
- Outgoing phase must have `status: complete` or `status: skipped`
- All required fields and history events must exist (skipped phases bypass these)

Use `validation_override: true` only when manually unblocking.

## Phase configuration

| Phase | Required fields | Required history events |
|-------|-----------------|------------------------|
| discovery | status | discovery_approved |
| spec | status, spec_file, approved, review_completed | spec_approved |
| design | status, design_doc, draft_ready, finalized, review_completed | design_draft_ready, design_review_complete, design_finalized |
| plan | status, plan_file, approved | plan_created, plan_approved |
| implement | status | implementation_complete |
| pr | status, pr_url, pr_number, ci_passed | pre_flight_checks_passed, pr_created |
| operational | status | operational_started |
| execute | status | execution_complete |
| document | status | documentation_complete |

## Allowed transitions

```
init -> discovery | spec | operational
discovery -> design
design -> plan
plan -> implement
spec -> implement
implement -> pr
pr -> complete
operational -> execute | document | complete
execute -> document | complete
document -> complete | pr
complete -> (terminal)
```

Note: there is intentionally **no `review` phase** in gcm. The
codex+gemini validation gate runs inside `implement.md` step 4 before
transitioning to `pr`.

## Phase files

`.pi/orchestrator/phases/`:
- `init.md`, `discovery.md`, `design.md`, `plan.md`
- `implement.md` (includes the codex+gemini validation gate)
- `pr.md`, `complete.md`
- `spec.md`, `operational.md`
- `status.md`, `blocked.md`

## Adding fields or phases

1. Update `WorkflowState` in `index.ts`.
2. Update the YAML Checkpoint section of the relevant phase file.
3. (Optional) Update `PHASE_CONFIG` for strict enforcement.
4. **Mirror the change in `.claude/commands/task/phases/<phase>.md`** to
   maintain Claude/Pi parity.

## See also

- `.claude/commands/task/orchestrate.md` - canonical Claude flow (if mirrored)
- `.claude/commands/task/phases/*.md` - Claude-side phase scripts
- `../../docs/` - gcm planning artifacts (`specs/`, `plans/`, `status/`)
