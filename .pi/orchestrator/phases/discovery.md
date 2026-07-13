# Phase: discovery

Frame the problem, understand existing code, identify approaches, and
choose one. Output is a Discovery Report (`docs/discovery/clo-XX.md`)
plus an updated PRD section if the issue lacked one.

Mirrors `.claude/commands/task/phases/discovery.md`.

## Required exit state

```yaml
phases:
  discovery:
    status: complete
    approved: true                      # optional - docs-only signal
    problem_framed: true                # optional - docs-only signal
    prd_exists: true                    # optional - docs-only signal
    prd_file: "docs/prds/clo-XX-<slug>.md"   # optional
    prd_created: true | false           # optional
    discovery_report: "docs/discovery/clo-XX.md"   # optional
    discovery_debt: []                  # optional - list of follow-up unknowns
    baseline_score: <int 0-10>          # optional
    approaches_identified: <int>        # optional
    approach_chosen: "<short label>"    # optional
```

History events required: `discovery_approved`.

## Step 1 - Branch

If `linear.branch_actual` is empty, create the branch:

```bash
git checkout main && git pull
git checkout -b feat/clo-XX-<short-slug>
```

After the branch exists, initialize session knowledge capture (deterministic,
survives worktree/branch teardown; no-ops if the CLI is not installed):

```bash
"$HOME/bin/session-knowledge" init
```

Record:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "discovery",
  action: "branch_created",
  details: "Branch feat/clo-XX-... checked out from main",
  linear_updates: { branch_actual: "feat/clo-XX-..." }
})
```

## Step 2 - Frame the problem

Read the Linear issue, design doc references, and any linked context.
Write 3-5 sentences that capture:

- WHO is affected and HOW today
- WHAT the current behaviour is vs WHAT we want
- WHY now (the trigger)

If the issue body has no PRD section, draft one and store it at
`docs/prds/clo-XX-<slug>.md`. Set `prd_created: true`.

## Step 3 - Explore existing code

Use `Grep`, `Glob`, and `Read` to locate the relevant modules. Capture
findings in `docs/discovery/clo-XX.md` under `## Existing code`.

Score the baseline 0-10 (10 = "no rewrite needed"). Anything below 7
should justify the rewrite in the report.

## Step 4 - Identify approaches

List at least two approaches. For each:

- One-line summary
- Pros (3 bullets max)
- Cons (3 bullets max)
- Effort estimate (S / M / L)
- Risk (low / medium / high)

## Step 5 - Choose

Pick one approach. Record `approach_chosen` and a 1-2 sentence reason.

If you cannot choose without user input, set `workflow.status = blocked`
and dispatch `phases/blocked.md`.

## Step 6 - Approval checkpoint

In Auto Mode, mark `approved: true` immediately if:

- All required exit fields are populated
- `discovery_debt` is empty OR every debt item has an explicit
  follow-up Linear issue planned
- The chosen approach is one of the identified approaches

Otherwise prompt the user.

## Step 7 - Persist and transition

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "discovery",
  action: "discovery_approved",
  details: "Approach: <label>. Baseline <n>/10. <approaches> approaches considered.",
  phase_updates: {
    status: "complete",
    approved: true,
    problem_framed: true,
    prd_exists: true,
    prd_file: "docs/prds/clo-XX-<slug>.md",
    prd_created: true,
    discovery_report: "docs/discovery/clo-XX.md",
    discovery_debt: [],
    baseline_score: 7,
    approaches_identified: 2,
    approach_chosen: "..."
  }
})

transition_phase({
  task_id: "CLO-XX",
  from_phase: "discovery",
  to_phase: "design"
})
```

## Linear update

Comment on the Linear issue:

```
mcp__linear__save_comment(
  issueId="CLO-XX",
  body="Discovery complete. Approach: <label>. Report: docs/discovery/clo-XX.md"
)
```

Move state `Todo -> In Progress` if not already.
