# Phase: init

Initialize a new CLO-XX workflow or resume an existing one. This phase is
implicit - the orchestrator runs it before dispatching the actual phase
file.

Mirrors `.claude/commands/task/phases/init.md` so a task started in
Claude can resume in pi (and vice-versa). Schema must stay identical.

## Step 1 - Parse arguments

The user invokes:

```
/task:orchestrate CLO-42 [--status|--ops|--spec|--skip-discovery]
```

Flags:

| Flag | Effect |
|---|---|
| `--status` | Show current state, do not dispatch phase. |
| `--spec` | Force `task_type=specification`. Skip discovery+design. |
| `--ops` | Force `task_type=operational`. Skip discovery+design+plan. |
| `--skip-discovery` | Development task that skips discovery (rare). |

If `--status` is set, jump to `phases/status.md`.

## Step 2 - Init or resume

Status file: `docs/status/clo-XX-workflow.yaml` (lowercase `clo-`).

### 2.1 Resume (file exists)

Read the file. If `workflow.status == "complete"` print a summary and stop
unless the user passed `--force-restart`. Otherwise dispatch the phase
named in `workflow.current_phase`.

### 2.2 Init (file does not exist)

1. Fetch the Linear issue:
   ```
   mcp__linear__get_issue(id="CLO-42")
   ```
2. Capture: title, url, description, priority, current state, blocks,
   blocked_by, suggested branch.
3. Classify task type (Step 2.3) unless overridden by `--spec` / `--ops`.
4. Pre-create the workflow file by calling `update_workflow_state` with
   the phase set to `init` (the orchestrator runtime initialises the
   skeleton on first call - see `index.ts::initializeWorkflow`).
5. Update Linear status to `Backlog -> Todo` if it is still `Backlog`.

### 2.3 Classify task type

| Signal | Type |
|---|---|
| Issue body has `**Type:** Specification` or `[spec]` label | `specification` |
| Issue body has `**Type:** Operational` or `[ops]` label | `operational` |
| Title starts with `Investigate`, `Audit`, `Document`, `Migrate`, `Configure` | `operational` |
| Title starts with `Add`, `Implement`, `Fix`, `Refactor` and ACs are mechanical | `specification` |
| Otherwise | `development` |

Set `task_type` and write `classification_reason` describing the signal.

### 2.4 Record classification and Linear metadata

Call:

```ts
update_workflow_state({
  task_id: "CLO-42",
  phase: "init",
  action: "init_classified",
  details: "Classified as <type>: <reason>",
  workflow_updates: {
    status: "active"
  },
  linear_updates: {
    team: "Cloud-ai",
    project: "gcm",
    status_at_start: "<linear status>",
    priority: "<linear priority>",
    branch_suggested: "<from linear>",
    blocks: [...],
    blocked_by: [...]
  },
  root_updates: {
    task_title: "...",
    task_url: "https://linear.app/cloud-ai/issue/clo-42/...",
    task_type: "<type>",
    classification_reason: "..."
  }
})
```

Do NOT set `workflow_updates.current_phase` here. The phase must stay
`"init"` until Step 3's `transition_phase` call - otherwise the
`from === currentPhase` check in the transition validator fails.

### 2.5 Project sync start

Only on NEW workflow (not resume). gcm tracks roadmap state in
`PROJECT.md`, `ROADMAP.md`, and `DEPENDENCIES.md` at the repo root.
These are mutated only through the `/project:sync` slash command, which
also enforces the WIP limit (max 3 active tasks) and blocker checks.

1. Invoke `/project:sync CLO-XX --start`.

2. The sync validates:
   - WIP limit (max 3 in-progress tasks in `PROJECT.md`).
   - Task is not blocked (per `DEPENDENCIES.md`).

3. On validation failure: `/project:sync` displays the issue. Stop and
   put the workflow into `blocked` state via `update_workflow_state`
   until the user resolves it.

4. On success, the sync writes:
   - `PROJECT.md`: task moved to "Active Work".
   - `ROADMAP.md`: task status changed to "In Progress".
   - `DEPENDENCIES.md`: task removed from "Unblocked & Ready".

5. Record the sync in the workflow YAML:

   ```ts
   update_workflow_state({
     task_id: "CLO-42",
     phase: "init",
     action: "project_sync_start",
     details: "PROJECT.md/ROADMAP.md/DEPENDENCIES.md synced via /project:sync CLO-42 --start"
   })
   ```

NEVER hand-edit `PROJECT.md`, `ROADMAP.md`, or `DEPENDENCIES.md` from a
phase script. Always go through `/project:sync`.

## Step 3 - Transition to the first real phase

Pick the first phase from the classified `task_type` and call
`transition_phase`:

| `task_type` | First phase |
|---|---|
| `development` | `discovery` |
| `specification` | `spec` |
| `operational` | `operational` |

```ts
transition_phase({
  task_id: "CLO-42",
  from_phase: "init",
  to_phase: "<discovery|spec|operational>"
})
```

`init` has no `PHASE_CONFIG` entry, so the validator skips the
required-fields/history-events checks. The remaining gates are the
`from === currentPhase` check (why Step 2.4 must not mutate
`current_phase`) and `to_phase` being in the allowed sets:
`ALLOWED_TRANSITIONS.init` and `TYPE_ALLOWED_PHASES[task_type]`. The
runtime auto-dispatches the destination phase markdown as a follow-up
prompt in the same session.

(`--ops`, `--spec`, and `--skip-discovery` short-circuit init.md
entirely - the slash command handler rewrites `current_phase` before
dispatch, so a different phase file runs instead of this one.)

## Runtime contract for every later phase

After this point, the loop is:

1. The agent runs the dispatched phase file's steps.
2. The phase file ends with a `transition_phase({...})` call.
3. The runtime auto-dispatches the next phase's file as a follow-up.

If `transition_phase` does not auto-dispatch (older builds of the
orchestrator extension), re-run `/task:orchestrate CLO-XX` to resume from
the new `current_phase`.

## Notes

- Branch creation happens in the first real phase (discovery/spec/ops),
  not here. The init phase only records `branch_suggested`.
- Never overwrite an existing workflow file unless `--force-restart` is
  passed.
- Always read Linear via `mcp__linear__*` tools. In pi these come from
  the `.pi/extensions/linear/` bridge; in Claude they come from the
  global Linear MCP config. The names match either way.
