# Phase: document

Documentation sub-phase of `operational`. Write up findings, decisions,
and follow-ups in the report file. The report IS the deliverable for
investigation-only tasks; for execution tasks it is the evidence trail.

Entered from `operational` (investigation-only) or `execute`
(post-action). Exits to `pr` (if tracked files changed) or `complete`.

## Required exit state

```yaml
phases:
  document:
    status: complete
```

History events required: `documentation_complete`.

## Step 1 - Choose the report location

Pick the directory that fits the work:

- `docs/operations/clo-XX-<slug>.md` for routine ops.
- `docs/audits/clo-XX-<slug>.md` for security / compliance reviews.
- `docs/migrations/clo-XX-<slug>.md` for migrations.
- `docs/investigations/clo-XX-<slug>.md` for one-off investigations.

## Step 2 - Write the report

Include at minimum:

- **Trigger** - why this work happened.
- **Scope** - what was in / out.
- **What was done** (for execute) or **what was found** (for audit /
  investigation).
- **Evidence** - file:line references, query results, log excerpts.
  Never paste customer prompts, API keys, or vault content; reference
  paths and IDs only.
- **Recommendations** - with severity (`blocker` / `major` / `minor`
  / `nit`) and owner / next step.
- **Follow-ups** - linked Linear issues for any deferred work.

## Step 3 - Persist and transition

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "document",
  action: "documentation_complete",
  details: "Report saved at docs/<flavour>/clo-XX-<slug>.md",
  phase_updates: { status: "complete" }
})
```

If the work touched tracked files (code, config, docs that need review):

```ts
transition_phase({
  task_id: "CLO-XX",
  from_phase: "document",
  to_phase: "pr"
})
```

Otherwise:

```ts
transition_phase({
  task_id: "CLO-XX",
  from_phase: "document",
  to_phase: "complete"
})
```

## Notes

- The merge step in `complete.md` becomes a no-op for non-PR tasks -
  record `merged_at: null` and the report path.
- Update Linear status `In Progress -> In Review -> Done` even when no
  PR is opened.
