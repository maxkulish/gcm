# Phase: design

Translate the chosen discovery approach into a concrete design document.
Run an AI design review, classify feedback, and finalize.

Mirrors `.claude/commands/task/phases/design.md`.

## Required exit state

```yaml
phases:
  design:
    status: complete
    design_doc: "docs/designs/clo-XX-<slug>.md"
    draft_ready: true
    discovery_context_used: true                 # optional - docs-only signal
    assumptions_surfaced: true
    assumptions:                                 # optional
      - text: "<one-line assumption>"
        confidence: "high" | "medium" | "low"
        verification: "<how/when this gets verified, or 'unverified'>"
    review_completed: true
    review_gemini: "docs/reviews/clo-XX-design-gemini.md"   # optional
    review_synthesis: "docs/reviews/clo-XX-design-synthesis.md"   # optional
    review_verdict: "approve" | "approve_with_changes" | "rework"  # optional
    applied_suggestions: []                      # optional
    flagged_suggestions: []                      # optional
    human_review_completed: true
    plannotator_annotations: ""                  # optional
    finalized: true
    token_usage:                                              # optional, observational
      - recorded_at: "2026-05-16T12:00:00Z"
        provider: "gemini"
        model: "gemini-3.5-flash"
        prompt_tokens: 0
        completion_tokens: 0
        task_label: "design-review"
```

`token_usage` is observational - not validated by `PHASE_CONFIG`. Append
entries via `update_workflow_state({ token_usage: [...] })` after each
AI-driven step (draft generation, review). The extension appends.

History events required: `design_draft_ready`,
`design_assumptions_surfaced`, `design_review_complete`,
`design_human_review_complete`, `design_finalized`.

## Status discipline (READ FIRST)

The `design` phase has 7 required exit-state fields. The orchestrator
validates them only when `phases.design.status` is set to `"complete"`.
Until then, the phase is in progress and partial state is expected.

Concretely:

- **Steps 1, 1.5, 2, 3, 4 - keep `status: "in_progress"`.** Each step
  populates one or more fields and emits one history event. Do NOT set
  `status: "complete"` in any of these calls.
- **Step 5 - and ONLY Step 5 - sets `status: "complete"`.** At that
  point all 6 supporting flags (`design_doc`, `draft_ready`,
  `assumptions_surfaced`, `review_completed`, `human_review_completed`,
  `finalized`) must already be populated from prior steps, and all 5
  required history events must already exist.

If you call `update_workflow_state` with `status: "complete"` before
those fields/events exist, the orchestrator emits "Cannot mark phase
complete - missing exit-state requirements: ...". That is the gate
working as intended; the fix is to set `status` back to `"in_progress"`
and finish the remaining steps, not to retry the same call.

## Step 0 - Consult prior lessons

Before drafting, grep `.pi/lessons/` for rules that apply to the
modules / subsystems this task will touch:

```bash
ls .pi/lessons/ 2>/dev/null
grep -l -i -e backend -e workflow -e conductor -e tasks -e apply_verify -e role .pi/lessons/ 2>/dev/null
```

Also search the central session capture (durable facts first, raw
`unverified` notes second - these include findings captured this task or by
a sibling task, including work done in the pi runtime):

```bash
"$HOME/bin/session-knowledge" search <keywords for the touched subsystems>
```

Adjust the keyword list to match what this task is about. Read every
matching file end-to-end. Hits become inputs to:

- The Assumptions section (Step 1.5) - phrase relevant lessons as
  high-confidence assumptions, citing the lesson reference
  (`.pi/lessons/<file>.md § L<n>`) in the `verification` field.
- The Test plan section - if a lesson names a recurring failure mode,
  the test plan must cover it.

If no lessons file matches, that is fine - move to Step 1. Do not
synthesize lessons or speculate; only cite what exists. Lessons are
the rationale store; do not paste their content into the design doc -
reference them by path and `L<n>` index.

## Step 1 - Generate the design draft

Run the draft workflow. It reads discovery outputs from the workflow YAML and
produces a first draft at `docs/designs/clo-XX-<slug>.md`:

```bash
lok run .lok/workflows/design-draft.toml \
  CLO-XX \
  <slug> \
  "<task title>" \
  --dir . --verbose
```

If lok is unavailable, fall back to writing the draft manually using the
7-section structure below, then skip to the soft gate.

**Soft gate** - open the draft, read it end-to-end, make any edits needed.
Then run this in a terminal to confirm you are satisfied:

```bash
DESIGN_DOC="docs/designs/clo-xx-<slug>.md"
printf '\nDraft at %s (%d lines).\nEdit it now, then press Enter to start the AI review pipeline, or Ctrl+C to abort: ' \
  "$DESIGN_DOC" "$(wc -l < "$DESIGN_DOC")"
read -r _
```

Ctrl+C at this point leaves the workflow YAML unchanged. Re-running Step 1
overwrites the draft.

Only after pressing Enter, record state:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "design",
  action: "design_draft_ready",
  details: "Draft generated by claude-designer at docs/designs/clo-XX-<slug>.md",
  phase_updates: {
    status: "in_progress",
    design_doc: "docs/designs/clo-XX-<slug>.md",
    draft_ready: true,
    draft_source: "claude-designer",
    discovery_context_used: true
  }
})
```

The 8-section structure the draft must contain (verify before pressing Enter):

- Problem (1 paragraph, citing discovery)
- Goals / Non-goals
- Architecture (modules, data flow, concrete types)
- Public API surface (Rust trait/struct signatures)
- **Assumptions** (see Step 1.5 - this section is mandatory; an empty
  list is acceptable but the section heading must be present)
- Test plan (unit, integration, manual)
- Migration / rollout
- Open questions

## Step 1.5 - Surface assumptions

Before the AI review fires, enumerate the assumptions the design rests
on. An assumption is anything that, if false, would invalidate the
design - not the implementation choices. Examples:

- "The conductor runs single-threaded per task; no concurrent step writes."
- "Workflow TOML files stay under 1 MB; we never need streaming parse."
- "Bedrock fallback is sufficient when the primary provider returns empty."
- "Linear ticket IDs map 1:1 to slices; no slice spans two tickets."

For each assumption, record:

| Field | Values | Meaning |
|---|---|---|
| `text` | one sentence | The assumption itself, phrased as a statement. |
| `confidence` | `high`, `medium`, `low` | How sure are we today. `low` = "this is a guess." |
| `verification` | one sentence or `unverified` | How / when the assumption gets confirmed (test, prod observation, follow-up ticket) or `unverified` if it cannot be cheaply checked now. |

Mirror the list under `## Assumptions` in the design doc AND into the
workflow YAML field `phases.design.assumptions`. The two MUST agree;
the synthesis review will fail the design otherwise.

Why this exists: most design rework after `implement` traces back to
an unstated assumption. Surfacing them here lets Gemini and the human
reviewer flag the wrong ones cheaply, before code lands.

When the list is genuinely empty (small, isolated change), record:

```yaml
phases:
  design:
    assumptions_surfaced: true
    assumptions: []
```

and explain in `details` why. Empty is allowed; absent is not.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "design",
  action: "design_assumptions_surfaced",
  details: "<n> assumptions surfaced (high: <n>, medium: <n>, low: <n>). See design doc § Assumptions.",
  phase_updates: {
    assumptions_surfaced: true,
    assumptions: [
      { text: "...", confidence: "high", verification: "..." }
    ]
  }
})
```

## Step 2 - AI design review

If `.lok/workflows/design-review.toml` exists:

```bash
lok run .lok/workflows/design-review.toml \
  docs/designs/clo-XX-<slug>.md \
  CLO-XX \
  "<task title>" \
  --dir . --verbose
```

This produces gemini + synthesis review files in `docs/reviews/`.

If lok tooling is unavailable in this repo, you may invoke the persona
directly via the agent: `pi run gemini-architect --input
docs/designs/clo-XX-<slug>.md`. If neither path works, set:

```yaml
review_completed: true
review_skip_reason: "<concrete reason>"
```

## Step 3 - Apply review feedback

Classify each suggestion:

| Class | Action |
|---|---|
| **Additive** (new test, doc clarification, edge case) | Apply immediately. Add to `applied_suggestions`. |
| **Refinement** (renames, signature tweaks) | Apply if cheap, else defer. |
| **Contradicts** the chosen approach | Do NOT apply. Add to `flagged_suggestions` with reason. |

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "design",
  action: "design_review_complete",
  details: "Review verdict: approve_with_changes. <n> applied, <m> flagged.",
  phase_updates: {
    review_completed: true,
    review_gemini: "docs/reviews/clo-XX-design-gemini.md",
    review_synthesis: "docs/reviews/clo-XX-design-synthesis.md",
    review_verdict: "approve_with_changes",
    applied_suggestions: ["..."],
    flagged_suggestions: [{ id: "...", reason: "..." }]
  },
  token_usage: [
    { provider: "gemini", model: "gemini-3.5-flash", prompt_tokens: <p>, completion_tokens: <c>, task_label: "design-review-gemini" },
    { provider: "claude", model: "claude-opus-4", prompt_tokens: <p>, completion_tokens: <c>, task_label: "design-review-synthesis" }
  ]
})
```

If lok printed token counts in its `--verbose` summary, pass them through.
If a step ran without counts (manual fallback, local Ollama), still emit an
entry with `prompt_tokens: 0, completion_tokens: 0` and a `task_label` that
makes the gap obvious. The accumulator is append-only.

## Step 4 - Human review via Plannotator annotate

The AI review iterations in Steps 2-3 caught what models can catch. This step
opens a Plannotator browser UI so a human can view the design doc and leave
inline annotations before code starts landing.

`/plannotator-annotate` opens the design doc in a browser, making it human
readable (proper formatting, syntax highlighting, clickable annotations).

Print a one-line summary of what changed in Step 3 (count of applied vs
flagged suggestions).

**Step 4a - Pre-flight: confirm plannotator is installed.**

Run via the Bash tool:

```bash
command -v plannotator
```

- **Exit 0 (binary found)**: continue to Step 4b.
- **Exit non-zero (binary missing)**: do NOT call `update_workflow_state`,
  do NOT auto-degrade to a manual-review fallback. Halt with this message
  and stop the phase:

  ```
  plannotator is required for the design human-review gate but is not
  installed on this machine. Install it from
  https://github.com/anthropics/plannotator (or your team's mirror), then
  re-run /task:orchestrate CLO-XX.

  If you intentionally want to skip this gate for THIS task only, re-run
  with validation_override: true on the next transition_phase.
  ```

**Step 4b - Invoke plannotator annotate (browser UI).**

The `/plannotator-annotate` skill runs this under the hood. Call the binary
directly so stdout and exit code are deterministic:

```bash
plannotator annotate docs/designs/clo-XX-<slug>.md
```

Behavior:

1. A browser tab opens with the rendered design doc.
2. The human reviewer reads it and leaves annotations inline.
3. Wait for the browser session to complete.
4. Capture any returned annotations.

**If the session returns annotations:**

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "design",
  action: "design_human_review_annotated",
  details: "<one-line summary of annotations, max 200 chars>",
  phase_updates: {
    human_review_completed: false,
    plannotator_annotations: "<full annotation text>"
  }
})
```

Then print the full annotations to the user and **STOP**. Do not call
`transition_phase`. Tell the user:

```
Plannotator returned annotations on the design. See
phases.design.plannotator_annotations. Edit
docs/designs/clo-XX-<slug>.md by hand, then re-run
/task:orchestrate CLO-XX to re-fire the gate.
```

**If the session closes without feedback (no annotations):**

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "design",
  action: "design_human_review_complete",
  details: "Plannotator annotate session closed with no annotations.",
  phase_updates: {
    human_review_completed: true
  }
})
```

Then proceed to Step 5.

**Re-entry on resume.** When the user re-runs `/task:orchestrate CLO-XX`
after editing, the design phase will dispatch again. At that point
`draft_ready: true` and `review_completed: true` are already set, so
skip Steps 1-3 and re-enter at Step 4 (re-run pre-flight, then re-fire
plannotator annotate on the edited doc).

## Step 5 - Finalize

Re-read the design doc end-to-end. Make sure:

- Public API signatures compile mentally
- Test plan is concrete enough for `plan` phase to enumerate sub-tasks
- All open questions are resolved or moved to follow-up issues

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "design",
  action: "design_finalized",
  details: "Design ready for plan phase",
  phase_updates: {
    status: "complete",
    finalized: true
  }
})

transition_phase({
  task_id: "CLO-XX",
  from_phase: "design",
  to_phase: "plan"
})
```

## Notes

- gcm has no separate `review` phase. The codex+gemini validation
  gate runs inside `implement.md` step 5 - that gate is for *code*, this
  step is for *design*. Both are required.
- If the design fundamentally changes scope, return to `discovery` via
  the user rather than forcing a transition.
