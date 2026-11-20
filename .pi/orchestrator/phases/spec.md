# Phase: spec

Specification-task path. Produce a focused 5-section spec document at
`docs/specs/YYYY-MM-DD-clo-XX-<slug>.md`, run AI review (lok pipeline), and
transition straight to `implement` (no design / plan phase).

Mirrors `.claude/commands/task/phases/spec.md`.

## Required exit state

```yaml
phases:
  spec:
    status: complete
    spec_file: "docs/specs/YYYY-MM-DD-clo-XX-<slug>.md"
    approved: true
    auto_approved: true | false        # optional
    auto_approval_reason: "..."        # optional - if auto_approved
    review_completed: true
    review_skip_reason: "..."          # optional - only if no review of this run can be read
    review_gemini: "docs/reviews/clo-XX-spec-review-gemini.md"   | null    # optional
    review_codex: "docs/reviews/clo-XX-spec-review-codex.md"   | null    # optional
    review_ollama: "..." | null        # optional - legacy key, read on resume only, never written
    review_synthesis: "docs/reviews/clo-XX-spec-review-synthesis.md" | null   # optional
    review_verdict: "approve" | "approve_with_changes" | "rework" | null   # optional
    review_applied: true | false       # optional
    applied_suggestions: []            # optional
    flagged_suggestions: []            # optional
    token_usage:                                              # optional, observational
      - recorded_at: "2026-05-16T12:00:00Z"
        provider: "gemini"
        model: "gemini-3.5-flash"
        prompt_tokens: 0
        completion_tokens: 0
        task_label: "spec-review"
```

History events required: `spec_approved`. Optional:
`phase_completed`, `spec_review_complete`.

`token_usage` is observational - not validated by `PHASE_CONFIG`. Append
entries via `update_workflow_state({ token_usage: [...] })` after any AI
dispatch. The extension appends; it does not overwrite.

## Step 1 - Branch

If `linear.branch_actual` is empty:

```bash
git checkout main && git pull
git checkout -b feat/clo-XX-<slug>
```

Record via `update_workflow_state` as in `discovery.md` Step 1.

## Step 2 - Write the spec

Path: `docs/specs/<today>-clo-XX-<slug>.md`. Use this 5-section structure:

```markdown
# CLO-XX <title>

**Status:** draft
**Type:** specification
**Linear:** https://linear.app/cloud-ai/issue/clo-xx/...
**Design context:** docs/design-docs/<relevant-doc>.md §<n> (or relevant design source)

## 1. Problem and goal
<3-5 sentences: what we are building and why>

## 2. Acceptance criteria
- [ ] AC1 ... (verifiable: `<command>`)
- [ ] AC2 ... (verifiable: `<command>`)
... (target ~10 ACs, every one with an explicit verification command)

## 3. Sub-tasks
### ST1 <verb> <component>
**Files:** src/...
**Tests:** tests/...
**Estimate:** S | M | L

### ST2 ...

## 4. Evaluation table
| # | Scenario | Input | Expected | Verification |
|---|---|---|---|---|
| 1 | ... | ... | ... | `cargo test ...` |

## 5. Edge cases
- Edge 1: ... -> handled by ...
- Edge 2: ... -> handled by ...
```

Save and record:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "spec",
  action: "spec_drafted",
  details: "Spec at docs/specs/<today>-clo-XX-<slug>.md (<n> ACs, <m> sub-tasks)",
  phase_updates: {
    status: "in_progress",
    spec_file: "docs/specs/<today>-clo-XX-<slug>.md"
  }
})
```

## Step 3 - AI spec review (if available)

If `.lok/workflows/spec-review.toml` exists, review the spec saved in
Step 2. Pass two arguments exactly:

- the spec path: the `phases.spec.spec_file` value recorded in Step 2
  (`docs/specs/<today>-clo-XX-<slug>.md`). The reviewers open this path,
  so it must name the saved file.
- the slug: the task id in lowercase, `clo-XX`. lok publishes the reports
  under this slug, and every path below uses the same slug.

```bash
lok run .lok/workflows/spec-review.toml \
  "<phases.spec.spec_file>" \
  "clo-XX" \
  "<task title>" \
  "<short description>" \
  "<labels>" \
  --dir . --verbose
```

A run that exits 0 publishes:

- `docs/reviews/clo-XX-spec-review-gemini.md`
- `docs/reviews/clo-XX-spec-review-codex.md`
- `docs/reviews/clo-XX-spec-review-claude-fallback.md` (only when the
  Claude fallback ran, that is when neither Gemini nor Codex was usable)
- `docs/reviews/clo-XX-spec-review-synthesis.md`

Check the results in this order:

1. **lok exit status.** If `lok run` exits non-zero, the reports of this
   run are not trustworthy: read no report file, record no report path,
   and record `review_skip_reason: "spec-review failed: <first failure
   line>"` with `review_completed: true`. If lok shows
   `REVIEW_PUBLISH_FAILED: see the run publish-failed file`, read
   `.lok/local/review-runs/clo-XX/publish-failed` before any rerun. Its
   line names the first report target that was not published (for
   example, a directory at that path) and the reason. Later targets were
   not attempted, and the next run of `spec-review` deletes the file.
2. **Synthesis first line.** Read the first line of
   `docs/reviews/clo-XX-spec-review-synthesis.md`:
   - `NO_REVIEWS_AVAILABLE`: Gemini, Codex and the Claude fallback all
     failed. Note "No AI reviews available" and go to Step 4.
   - `SYNTHESIS_FAILED: <reason>`: the synthesis is not usable. Use the
     usable individual reports it lists, and read no `Priority Actions`
     from the synthesis.
   - Anything else: the synthesis passed its check. Read
     `Priority Actions` from it.
3. **Individual reports.** A report has failed when its first line starts
   with `REVIEW_FAILED:`. Test the first line only: a usable review can
   quote the text `REVIEW_FAILED` in its body. Note which reviewer failed
   and continue with the usable reports.
4. **Claude fallback.** Read
   `docs/reviews/clo-XX-spec-review-claude-fallback.md` only when the
   synthesis `**Reviewers**:` line names `Claude (fallback`. Otherwise the
   file can be left from an earlier run. When the fallback is the only
   usable report, take its `Actionable Feedback` section as the review
   feedback and name "Claude fallback" as the source.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "spec",
  action: "spec_review_complete",
  details: "Verdict: <verdict>. Applied: <n>. Flagged: <m>.",
  phase_updates: {
    review_completed: true,
    review_gemini: "docs/reviews/clo-XX-spec-review-gemini.md",
    review_codex: "docs/reviews/clo-XX-spec-review-codex.md",
    review_synthesis: "docs/reviews/clo-XX-spec-review-synthesis.md",
    review_verdict: "<verdict>",
    review_applied: true,
    applied_suggestions: [...],
    flagged_suggestions: [...]
  },
  token_usage: [
    { provider: "gemini", model: "<model from the report Reviewer line>", prompt_tokens: <p>, completion_tokens: <c>, task_label: "spec-review-gemini" },
    { provider: "codex", model: "<model from the report Reviewer line>", prompt_tokens: <p>, completion_tokens: <c>, task_label: "spec-review-codex" }
  ]
})
```

Token counts come from the `lok run ... --verbose` summary line printed by
each provider. If the run did not report counts, pass `0` and add the
suffix `-no-count` to the `task_label` (for example
`spec-review-codex-no-count`) so the gap is visible later.

**Resume with a legacy key.** A workflow started before this change can
have `phases.spec.review_ollama` and no `review_codex`. Read the
`review_ollama` path as the second reviewer's report. Do not write
`review_ollama` in a new run.

If `.lok/workflows/spec-review.toml` is not present (current lok state),
record the skip:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "spec",
  action: "spec_review_skipped",
  details: "AI review tooling unavailable; marking review_completed=true",
  phase_updates: {
    review_completed: true,
    review_skip_reason: "No .lok/workflows/spec-review.toml present"
  }
})
```

## Step 4 - Approval checkpoint

Auto Mode auto-approves if:

- All required exit fields populated
- Every AC has an explicit verification command
- All sub-tasks reference real files / modules
- The task is mechanically testable (no architecture decisions hiding
  inside the spec)

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "spec",
  action: "spec_approved",
  details: "<auto_approval_reason or human approval note>",
  phase_updates: {
    status: "complete",
    approved: true,
    auto_approved: true,
    auto_approval_reason: "..."
  }
})
```

## Step 5 - Transition

Specification tasks skip `plan` entirely - the spec's sub-tasks ARE the
plan.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "spec",
  action: "phase_completed",
  details: "Transitioning spec -> implement. Skipping plan phase."
})

transition_phase({
  task_id: "CLO-XX",
  from_phase: "spec",
  to_phase: "implement"
})
```

The `implement` phase will still run the codex+gemini validation gate.

## Notes

- If the spec turns out to require architecture decisions, abort: set
  `task_type = development` and route to `discovery`/`design`.
- The spec file lives under `docs/specs/`, matching the gcm repo
  convention documented in `AI-AGENTS.md` and the Claude `task/spec.md`
  flow.
