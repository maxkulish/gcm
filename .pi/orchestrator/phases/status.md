# Phase: status

Read-only display of the current workflow state. Triggered by
`/task:orchestrate CLO-XX --status`. No state writes are made here.

Mirrors `.claude/commands/task/phases/status.md`.

## Behaviour

1. Load `docs/status/clo-XX-workflow.yaml`.
2. If the file does not exist, print:
   ```
   No workflow state for CLO-XX. Run `/task:orchestrate CLO-XX` to start.
   ```
   and exit.
3. Otherwise render a status block, then exit. Do NOT call
   `transition_phase` or `update_workflow_state`.

## Display format - development task

```
CLO-XX <task_title>
Type:        development
Linear:      <task_url>           (state at start: <linear.status_at_start>)
Branch:      <linear.branch_actual or "(not yet created)">
Status:      <workflow.status>
Phase:       <workflow.current_phase>     (<percent>%)
Updated:     <workflow.updated_at>

Phases
  discovery   <icon> <status>     <bullets>
  design      <icon> <status>     <bullets>
  plan        <icon> <status>     <bullets>
  implement   <icon> <status>     <commits count>, codex_verdict=<...>
  pr          <icon> <status>     PR #<n>: <url>, ci=<ok|pending|fail>
  complete    <icon> <status>     merged_at=<...>

History (last 5)
  <timestamp>  <action>  <one-line details>
```

`<icon>` mapping:

| Status | Icon |
|---|---|
| `pending` | `[ ]` |
| `in_progress` | `[~]` |
| `complete` | `[x]` |
| `skipped` | `[-]` |
| `blocked` | `[!]` |

`<percent>` is computed as `complete_phases / total_required_phases * 100`,
rounded. Skipped phases count as complete.

## Display format - specification task

```
CLO-XX <task_title>
Type:        specification
Linear:      <task_url>
Branch:      <linear.branch_actual>
Status:      <workflow.status>
Phase:       <workflow.current_phase>     (<percent>%)

Phases
  spec        <icon> <status>     spec_file=<...>, verdict=<...>
  implement   <icon> <status>     codex_verdict=<...>
  pr          <icon> <status>     PR #<n>
  complete    <icon> <status>

History (last 5)
  ...
```

Discovery / design / plan are listed as `[-] skipped` if their phase
block has `status: skipped`.

## Display format - operational task

```
CLO-XX <task_title>
Type:        operational
Linear:      <task_url>
Branch:      <linear.branch_actual or "(none, read-only)">
Status:      <workflow.status>
Phase:       <workflow.current_phase>     (<percent>%)

Phases
  operational <icon> <status>     started=<timestamp>
  execute     <icon> <status>     <details>
  document    <icon> <status>     <report path>
  pr          <icon> <status>     <only if code changed>
  complete    <icon> <status>

History (last 5)
  ...
```

## Notes

- This phase is purely informational. It must not write to the YAML or
  call any Linear MCP mutator.
- If you need a structured (machine-readable) status, dispatch with
  `--json` (future flag): print the YAML as-is and exit.
