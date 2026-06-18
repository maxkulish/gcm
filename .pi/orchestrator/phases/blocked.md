# Phase: blocked

Pseudo-phase used when work cannot proceed. The workflow's
`current_phase` does NOT change to `blocked` - that is a `status`, not a
phase. This file documents how to record and clear blockers.

Mirrors `.claude/commands/task/phases/blocked.md`.

## When to enter

Set `workflow.status = blocked` when:

- A required tool is missing (e.g. `codex` binary not installed)
- An external dependency is unresolved (e.g. blocked-by Linear issue
  still open)
- The user asked you to pause
- A test reveals an upstream bug that is out of scope
- AI review surfaces a blocker that cannot be resolved in this session

## Recording the block

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "<current_phase>",
  action: "workflow_blocked",
  details: "Blocker: <one-line>. Mitigation tried: <...>. Next: <what unblocks>.",
  workflow_updates: { status: "blocked" }
})
```

Update Linear:

```
mcp__linear__save_comment(
  issueId="CLO-XX",
  body="Blocked: <one-line>. Waiting on <CLO-YYY | external thing>."
)
mcp__linear__save_issue(id="CLO-XX", state="In Progress")
```

If `linear.blocked_by` was empty when the block surfaced, append the
real blocker:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "<current_phase>",
  action: "blocker_recorded",
  details: "Added <blocker> to linear.blocked_by",
  linear_updates: {
    blocked_by: [...existing, "<blocker description>"]
  }
})
```

## Project sync block

gcm tracks roadmap state in `PROJECT.md`, `ROADMAP.md`, and
`DEPENDENCIES.md` via the `/project:sync` slash command. If a blocker
needs to be reflected in those aggregation files (for example, the
task moves from "Active Work" back to "Blocked"), run:

```
/project:sync CLO-XX --blocked
```

If `/project:sync` is not available in the current environment (no
slash-command runtime), the blocker is still canonical in the workflow
YAML and on the Linear issue - leave a note in `details` that the
aggregation files were not synced, and re-run the sync after the
blocker is cleared.

## Clearing the block

When the blocker resolves:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "<current_phase>",
  action: "workflow_unblocked",
  details: "Blocker cleared: <how>. Resuming <phase>.",
  workflow_updates: { status: "active" },
  linear_updates: {
    blocked_by: []     // or remove the specific entry
  }
})
```

Then re-dispatch the original phase by re-running
`/task:orchestrate CLO-XX`. The orchestrator reads `workflow.status` and
`workflow.current_phase` to resume.

## Hard rules

- NEVER call `transition_phase` to a "blocked" phase - blocked is a
  status, not a phase. The `ALLOWED_TRANSITIONS` map intentionally has
  no `blocked` entries.
- NEVER silently skip a blocker. The status YAML is the source of truth
  for both Claude and pi - if it does not say `blocked`, future
  invocations will charge ahead.
- ALWAYS record what would unblock the work, in concrete terms (a
  Linear ID, a binary install, a user decision). "TBD" is not a
  mitigation.
