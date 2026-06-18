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
    review_skip_reason: "..."          # optional - only if review tooling unavailable
    review_gemini: "docs/reviews/clo-XX-spec-gemini.md"   | null    # optional
    review_ollama: "docs/reviews/clo-XX-spec-ollama.md"   | null    # optional
    review_synthesis: "docs/reviews/clo-XX-spec-synthesis.md" | null   # optional
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

If `.lok/workflows/spec-review.toml` exists:

```bash
lok run .lok/workflows/spec-review.toml \
  specs/<today>-clo-XX-<slug>.md \
  CLO-XX \
  "<task title>" \
  "<short description>" \
  "<labels>" \
  --dir . --verbose
```

Outputs:

- `docs/reviews/clo-XX-spec-gemini.md`
- `docs/reviews/clo-XX-spec-ollama.md`
- `docs/reviews/clo-XX-spec-synthesis.md`

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "spec",
  action: "spec_review_complete",
  details: "Verdict: <verdict>. Applied: <n>. Flagged: <m>.",
  phase_updates: {
    review_completed: true,
    review_gemini: "docs/reviews/clo-XX-spec-gemini.md",
    review_ollama: "docs/reviews/clo-XX-spec-ollama.md",
    review_synthesis: "docs/reviews/clo-XX-spec-synthesis.md",
    review_verdict: "<verdict>",
    review_applied: true,
    applied_suggestions: [...],
    flagged_suggestions: [...]
  },
  token_usage: [
    { provider: "gemini", model: "gemini-3.5-flash", prompt_tokens: <p>, completion_tokens: <c>, task_label: "spec-review-gemini" },
    { provider: "ollama", model: "qwen-coder", prompt_tokens: <p>, completion_tokens: <c>, task_label: "spec-review-ollama" }
  ]
})
```

Token counts come from the `lok run ... --verbose` summary line printed by
each provider. If the run did not report counts (local Ollama may omit
them), pass `0` and add `task_label: "spec-review-ollama-no-count"` so the
gap is visible later.

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
