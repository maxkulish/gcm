# Phase: execute

Execution sub-phase of `operational`. Run the actual operation (migration,
config change, audit script) and record what happened. The deliverable is
the side-effect on the system plus an evidence trail in the report file.

Entered from `operational`. Exits to `document` (most cases) or
`complete` (if the report was already written in-flight).

## Required exit state

```yaml
phases:
  execute:
    status: complete
```

History events required: `execution_complete`.

## Step 1 - Pre-checks

Before any irreversible action, confirm:

- The plan / scope written in `operational` step 1 is still current.
- Any required snapshot, backup, or rollback path exists.
- Destructive operations (`DROP`, `terragrunt apply -auto-approve`,
  force-push) are explicitly authorized.

If a destructive step is unauthorized, stop and dispatch
`phases/blocked.md`.

## Step 2 - Run

Execute the operation. Capture stdout/stderr to the report file (or a
linked log path). For each meaningful sub-step:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "execute",
  action: "execution_in_progress",
  details: "<step>; <intermediate state or output excerpt>"
})
```

## Step 3 - Verify

Show that the operation achieved its intended state. Examples:

- Migration: row counts before / after, sample query results.
- Config change: feature behaviour observed in the target environment.
- Audit script: produced report file with finding count.

## Step 4 - Persist and transition

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "execute",
  action: "execution_complete",
  details: "<what was done>; <verification evidence>",
  phase_updates: { status: "complete" }
})

transition_phase({
  task_id: "CLO-XX",
  from_phase: "execute",
  to_phase: "document"
})
```

If the report was already finalised in-flight, you may transition
directly to `complete` instead.

## Notes

- The codex+gemini validation gate is **not** required for `execute`
  unless code under `src/` changed; in that case open a `pr` after
  `document`.
- Never paste credentials, API keys, or vault content into the report
  or history details. Reference paths and run IDs only.
