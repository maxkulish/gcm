# Phase: complete

Merge the PR, sync project aggregation files (if any), and close the
workflow. Mirrors `.claude/commands/task/phases/complete.md`.

## Required exit state

```yaml
phases:
  complete:
    status: complete
    aggregation_files_updated: true | false
    merged_at: "<ISO-8601>"
    lessons_learned: []                  # optional, cross-task memory
    lessons_file: ".pi/lessons/clo-XX-<slug>-lessons.md"  # optional
    lessons_skip_reason: "<reason>"      # optional, only if no lessons extracted

workflow:
  current_phase: complete
  status: complete
```

`lessons_learned` and `lessons_file` are optional and not gated by
`PHASE_CONFIG`. They feed the cross-task memory layer documented in
`.pi/AGENTS.md` § "Cross-task memory". Future tasks grep `.pi/lessons/`
during design + implement to surface prior decisions before drafting.

History events required: `workflow_complete`. Optional: `pr_merged`,
`project_sync_complete`, `lessons_extracted`.

## Step 1 - Merge the PR

```bash
gh pr merge <n> --squash --delete-branch
```

Capture the merge commit SHA from the output.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "complete",
  action: "pr_merged",
  details: "PR #<n> merged. Merge commit <sha>.",
  phase_updates: {
    merged_at: "<ISO-8601>",
    merge_commit: "<sha>"
  }
})
```

Also update the `pr` phase block so both records agree:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "pr_merged",
  details: "Merge commit <sha>",
  phase_updates: {
    merged_at: "<ISO-8601>",
    merge_commit: "<sha>"
  }
})
```

## Step 2 - Local cleanup (with worktree safety guard)

Before any cleanup, check for untracked or uncommitted files on the
feature branch. `git branch -D` silently destroys uncommitted work.

```bash
# Check for dirty files before switching away
if git status --porcelain | grep -q .; then
  echo "WARNING: dirty working tree detected. Uncommitted files will be lost."
  echo ""
  git status --short
  echo ""
  echo "Options:"
  echo "  1. Commit these files to the feature branch and push (if they belong to this task)"
  echo "  2. Stash them: git stash push -m \"clo-XX: uncommitted artifacts\""
  echo "  3. Abort and inspect manually"
  echo ""
  echo "Refusing to run git branch -D with dirty files."
  exit 1
fi

git checkout main
git pull
git branch -D feat/clo-XX-<slug>     # safe to force-delete — working tree was clean
```

⚠️ **Never use `-D` (force delete) without checking `git status --porcelain`
first.** The force flag bypasses Git's "unmerged branch" safety check.
Untracked files (`.pi/lessons/*`, `docs/status/*`) are especially
dangerous because they are not tracked by any branch and `git checkout`
carries them silently — but `git branch -D` after checkout does not
error. The file is simply orphaned.

## Step 3 - Project sync complete

gcm tracks roadmap state in `PROJECT.md`, `ROADMAP.md`, and
`DEPENDENCIES.md` at the repo root. The merge must be reflected there
through the `/project:sync` slash command, NEVER by hand-editing the
files.

1. Invoke `/project:sync CLO-XX --complete`.

2. The sync writes:
   - `PROJECT.md`: task moved from "Active Work" to "Completed".
   - `ROADMAP.md`: task status changed to "Done".
   - `DEPENDENCIES.md`: downstream tasks that this one unblocks move
     into "Unblocked & Ready".

3. Record the sync in the workflow YAML:

   ```ts
   update_workflow_state({
     task_id: "CLO-XX",
     phase: "complete",
     action: "project_sync_complete",
     details: "PROJECT.md/ROADMAP.md/DEPENDENCIES.md synced via /project:sync CLO-XX --complete",
     phase_updates: {
       aggregation_files_updated: true
     }
   })
   ```

If `/project:sync` fails (e.g. uncommitted aggregation-file diffs), do
NOT proceed to Step 4. Fix the sync first, then retry.

## Step 4 - Linear

```
mcp__linear__save_issue(id="CLO-XX", state="Done")
mcp__linear__save_comment(
  issueId="CLO-XX",
  body="Merged in <sha>. Workflow YAML: docs/status/clo-XX-workflow.yaml"
)
```

## Step 4.5 - Extract lessons (cross-task memory)

Survey the workflow YAML one last time for durable rules other tasks
would benefit from. A "lesson" is something that, if a sibling task had
known it before starting, would have changed an early decision. It is
NOT a recap of what was implemented and NOT a list of files touched.

Candidate signals - scan each before deciding the list is empty:

| Signal | Where to look | Why it matters |
|---|---|---|
| Design assumptions that turned out `violated` or `untested` | `phases.implement.assumptions_revalidated_details` | A wrong assumption may bite sibling tasks in the same area. |
| Validation-gate fix iterations (`validation_fix_iteration_count > 0`) | `phases.implement` | The class of bug that codex/gemini caught is likely recurrent. |
| Flagged review suggestions the team intentionally declined | `phases.design.flagged_suggestions`, `phases.implement.flagged_suggestions` | Recording the rationale prevents the next task from re-litigating the same call. |
| Plannotator annotations on the design | `phases.design.plannotator_annotations` | Human-caught issues that the AI reviewers missed point to gaps in the rubric. |
| Surprising findings in discovery | `phases.discovery.findings` | A constraint or sibling system the project did not previously know about. |
| Bot-reviewer / PR incidents | `phases.pr` history events, late comments | Feeds `.pi/lessons/pr-review-failures.md` directly. |

For each lesson, decide which file it belongs in:

1. **Topic file** under `.pi/lessons/` if the rule is durable and likely
   to apply across tasks (e.g. `pr-review-failures.md`,
   `workflow-toml-conventions.md`). Append a new numbered section -
   `L<n> - <one-line rule>` - matching the existing format:
   `Source incident`, `Rule`, `How to apply`.
2. **Per-task file** at `.pi/lessons/clo-XX-<slug>-lessons.md` if the
   lesson is too narrow for a topic file but still worth keeping
   discoverable. Use the same `L<n>` block format.

If no candidate signals fired and the survey is genuinely empty, that
is acceptable - record the skip rather than skipping the step:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "complete",
  action: "lessons_extracted",
  details: "Survey complete; no durable lessons surfaced.",
  phase_updates: {
    lessons_learned: [],
    lessons_skip_reason: "No violated assumptions, no fix iterations, no flagged suggestions, no plannotator annotations."
  }
})
```

If lessons were extracted, **write the file now** but do **not** commit
it to `main` directly. It will ride in the post-merge docs PR (Step 6
below).

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "complete",
  action: "lessons_extracted",
  details: "<n> lessons recorded. File: .pi/lessons/<file>.md",
  phase_updates: {
    lessons_learned: [
      "L1 - <one-line rule>",
      "L2 - <one-line rule>"
    ],
    lessons_file: ".pi/lessons/<file>.md"
  }
})
```

Confidentiality reminder: `.pi/lessons/*.md` is committed and may
eventually be public-ish. Never paste vault content, Linear ticket
bodies, or customer names into a lesson. Reference file:line and ticket
IDs only - the same rule that applies to reviewer personas.

If the lesson updates a topic file rather than creating a new one,
include both paths in `details` (e.g. "Appended L6 to
`.pi/lessons/pr-review-failures.md`") and set `lessons_file` to the
topic file path.

## Step 5 - Post-merge docs PR (workflow YAML + lessons)

The workflow YAML and any per-task lessons file are authored *after*
the code PR is merged. Do **not** commit them directly to `main` (that
breaks in worktree setups and on protected branches). Instead, open a
small follow-up PR.

```bash
# Create a fresh branch from the just-merged main
git fetch origin main
git checkout -b feat/clo-XX-<slug>-workflow-docs origin/main
git add docs/status/clo-XX-workflow.yaml
git add .pi/lessons/clo-XX-<slug>-lessons.md      # if present
git commit -m "docs(CLO-XX): workflow complete + lessons"
git push -u origin feat/clo-XX-<slug>-workflow-docs

gh pr create \
  --title "docs(CLO-XX): workflow complete and lessons" \
  --body "Post-merge documentation for CLO-XX." \
  --base main

gh pr merge <n> --merge --delete-branch
```

Record the docs PR in the workflow state under `phases.complete.docs_pr`:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "complete",
  action: "docs_pr_merged",
  details: "Docs PR #<n> merged. Files: docs/status/clo-XX-workflow.yaml, .pi/lessons/...",
  phase_updates: {
    docs_pr_url: "<url>",
    docs_pr_number: <n>,
    docs_pr_merge_commit: "<sha>"
  }
})
```

If there are **no lessons** and **no workflow YAML changes** relative
to `origin/main`, the entire post-merge docs PR can be skipped.

## Step 6 - Mark workflow complete

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "complete",
  action: "workflow_complete",
  details: "Task CLO-XX fully completed. PR #<n> merged. <unblocks list>.",
  phase_updates: { status: "complete" },
  workflow_updates: { status: "complete" }
})
```

The orchestrator runtime treats `complete` as terminal - no further
`transition_phase` call is allowed.

## Notes

- If `aggregation_files_updated` is false, record a concrete reason -
  do not leave it null.
- For specification / operational tasks the same flow applies; only the
  earlier phases differ.
