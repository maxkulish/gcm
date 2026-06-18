# Phase: plan

Decompose the finalized design into ordered, mechanically testable
sub-tasks. Output is a Plan document (`docs/plans/clo-XX-<slug>.md`).

Mirrors `.claude/commands/task/phases/plan.md`.

## Required exit state

```yaml
phases:
  plan:
    status: complete
    plan_file: "docs/plans/clo-XX-<slug>.md"
    approved: true
```

History events required: `plan_created`, `plan_approved`.

## Step 1 - Build the plan document

Pi has no `/plan:create` slash command. Build the plan manually at
`docs/plans/clo-XX-<slug>.md` using this structure:

```markdown
# Plan: CLO-XX <title>

## Context
- Design: docs/designs/clo-XX-<slug>.md
- Discovery: docs/discovery/clo-XX.md (if any)
- Linear: https://linear.app/cloud-ai/issue/clo-xx/...

## Sub-tasks

### ST1 <verb> <component>
**Files:** src/...
**Acceptance:** `cargo test <test_name>` passes / pre-merge gate green
**Estimate:** S | M | L

### ST2 ...

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (fmt + clippy + test)

## Risks
- ...
```

Sub-tasks must be:

- Independently testable (one acceptance command per sub-task)
- Ordered so each builds on the previous
- Sized so the largest is at most one focused session

## Step 2 - Record creation

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "plan",
  action: "plan_created",
  details: "Plan with <n> sub-tasks at docs/plans/clo-XX-<slug>.md",
  phase_updates: {
    status: "in_progress",
    plan_file: "docs/plans/clo-XX-<slug>.md"
  }
})
```

## Step 3 - Approval checkpoint

In Auto Mode, mark `approved: true` immediately if:

- Every sub-task has a concrete acceptance command
- Sub-tasks reference real files / modules
- The pre-merge gate is `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (or a documented superset)

Otherwise prompt the user.

## Step 4 - Persist and transition

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "plan",
  action: "plan_approved",
  details: "Plan approved. <n> sub-tasks queued.",
  phase_updates: {
    status: "complete",
    approved: true
  }
})

transition_phase({
  task_id: "CLO-XX",
  from_phase: "plan",
  to_phase: "implement"
})
```

## Notes

- Specification tasks usually skip this phase. If `task_type ==
  "specification"` and the spec already enumerates sub-tasks, transition
  `spec -> implement` directly.
- If the plan reveals that the design is incomplete, return to `design`
  via user confirmation.
