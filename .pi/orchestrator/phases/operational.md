# Phase: operational

Operational tasks (audits, investigations, configuration, migrations,
documentation) where the primary deliverable is a status / report
document, not source code. The status YAML and the report ARE the
deliverable; a PR is opened only if code changed.

Mirrors `.claude/commands/task/phases/operational.md`.

## Required exit state

```yaml
phases:
  operational:
    status: complete
    operational_started: true        # optional - implicit via history
```

History events required: `operational_started`. Followed by `execute`
or `document` phases as needed.

## Allowed transitions

```
operational -> execute | document | complete
execute -> document | complete
document -> complete | pr
```

`pr` is only entered if the operational work produced code changes.

## Step 1 - Frame the work

Read the Linear issue. Capture in 3-5 sentences:

- Trigger (incident, audit, request)
- Scope (what is in / out)
- Done definition (what observable state means "complete")

Save as `docs/operations/clo-XX-<slug>.md` (or `docs/audits/...`,
`docs/migrations/...` depending on flavour).

## Step 2 - Branch (only if code might change)

If the work might touch tracked files, create a branch. Otherwise stay
on main and operate read-only.

## Step 3 - Mark phase started

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "operational",
  action: "operational_started",
  details: "<trigger>; scope: <one-line>",
  phase_updates: { status: "in_progress" }
})
```

## Step 4 - Execute

For execution-heavy tasks (running migrations, configuring infra,
applying changes):

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "execute",
  action: "execution_in_progress",
  details: "Running <thing>. <intermediate state>"
})
```

When done:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "execute",
  action: "execution_complete",
  details: "<what happened>; <verification>",
  phase_updates: { status: "complete" }
})

transition_phase({
  task_id: "CLO-XX",
  from_phase: "operational",
  to_phase: "execute"
})
```

For investigation-only tasks where no execution happens, you may skip
`execute` and move straight to `document`.

## Step 5 - Document

Write up findings in the report file. For audits include:

- What was checked
- What was found
- Recommended actions (with severity)

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "document",
  action: "documentation_complete",
  details: "Report saved at docs/operations/clo-XX-<slug>.md",
  phase_updates: { status: "complete" }
})

transition_phase({
  task_id: "CLO-XX",
  from_phase: "execute",          # or operational, depending on prior
  to_phase: "document"
})
```

## Step 6 - PR or complete

If the work touched tracked files (config, code, docs that need review):

```ts
transition_phase({
  task_id: "CLO-XX",
  from_phase: "document",
  to_phase: "pr"
})
```

Then follow `pr.md`. If the work was investigation-only:

```ts
transition_phase({
  task_id: "CLO-XX",
  from_phase: "document",       # or operational, or execute
  to_phase: "complete"
})
```

Then follow `complete.md`. The merge step in `complete.md` becomes a
no-op for non-PR tasks - just record `merged_at: null` and the report
path.

## Notes

- Update Linear status `Backlog -> In Progress -> In Review -> Done`
  even when no PR is opened.
- The codex+gemini validation gate is **not required** for operational
  tasks unless code changes are involved.
