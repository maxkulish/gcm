# Phase: implement

Land the plan's sub-tasks one by one. Run the codex+gemini validation
gate as the final pre-PR step. The validation gate is **embedded in this
phase** - gcm has no separate `review` phase.

Mirrors `.claude/commands/task/phases/implement.md`.

## Required exit state

**Every field below is mandatory.** `status: complete` is only legal once
the validation gate (Step 4) has produced all three review files AND the
synthesis verdict is `PASS` or `PASS_WITH_NOTES` (legacy: `approve` /
`approve_with_changes`) with the single fix iteration applied. See Step
4.6 for the hard checklist that gates Step 5.

```yaml
phases:
  implement:
    status: complete
    commits: ["abc123", "def456"]   # optional but recommended
    assumptions_revalidated: true
    assumption_outcomes:                          # optional
      - text: "<assumption text - must match a design.assumptions entry>"
        outcome: "held" | "violated" | "untested"
        evidence: "<file:line, test name, or prod observation>"
    codex_validated: true
    codex_verdict: "PASS" | "PASS_WITH_NOTES" | "FAIL"   # optional - legacy synonyms: approve | approve_with_changes | rework
    codex_report: "docs/reviews/clo-XX-codex-validation.md"
    gemini_validation_report: "docs/reviews/clo-XX-gemini-validation.md"
    validation_synthesis_report: "docs/reviews/clo-XX-validation-synthesis.md"
    validation_synthesis_verdict: "PASS" | "PASS_WITH_NOTES" | "FAIL"   # legacy synonyms: approve | approve_with_changes | pivot | rework
    validation_fix_iteration_count: 0 | 1   # optional
    token_usage:                                  # optional, observational
      - recorded_at: "2026-05-16T12:00:00Z"
        provider: "codex"
        model: "gpt-5.5"
        prompt_tokens: 0
        completion_tokens: 0
        task_label: "pre-pr-validation-codex"
```

`token_usage` is observational - not validated by `PHASE_CONFIG`. Append
entries via `update_workflow_state({ token_usage: [...] })` after the
`pre-pr-validation` workflow run (Step 4) and after any synthesis or
fix-iteration dispatch. The extension appends; it does not overwrite.

History events required: `implementation_complete`,
`assumptions_revalidated`, `codex_validation_complete`.

**Anti-pattern:** opening the PR before Step 4.6 passes. If you find
yourself thinking "I'll just push the PR and address validation comments
in review", stop. That is the failure mode this gate exists to prevent.
Codex/Gemini findings are pre-PR blockers, not review-cycle suggestions.

## Step 0 - Consult prior lessons

Before landing the first sub-task, grep `.pi/lessons/` for rules that
apply to the modules this task will touch:

```bash
ls .pi/lessons/ 2>/dev/null
grep -l -i -e backend -e workflow -e conductor -e tasks -e apply_verify -e role -e pr-review .pi/lessons/ 2>/dev/null
```

Adjust the keyword list to match the touched modules. Read every
matching file end-to-end. Hits become test-plan inputs:

- If a lesson names a recurring failure mode (e.g.
  `pr-review-failures.md § L1` - "CI absence != bot absence"), the
  sub-task that lands the relevant code path must include a test or a
  gate that prevents the same class of failure.
- Lessons concerning the PR phase (bot timing, reply trailer) are
  enforced in `pr.md`, not here - but if a lesson affects
  implementation (e.g. migration safety, retry semantics), it lands as
  a test in this phase.

If no lessons match, move to Step 1. Do not invent lessons; cite only
what exists.

## Step 1 - Land sub-tasks

For each sub-task ST1..STN in `docs/plans/clo-XX-<slug>.md`:

1. Implement the changes in the named files.
2. Run the sub-task's acceptance command (usually `cargo test ...`).
3. If green, commit:
   ```
   git add -A
   git commit -m "feat(CLO-XX): <ST verb> <component>"
   ```
4. Append the commit SHA to `phases.implement.commits`:
   ```ts
   update_workflow_state({
     task_id: "CLO-XX",
     phase: "implement",
     action: "subtask_complete",
     details: "ST1 landed: <description>. Commit <sha>",
     phase_updates: { commits: [...existing, "<sha>"] }
   })
   ```

If a sub-task fails after a reasonable attempt, set `workflow.status =
blocked` and dispatch `phases/blocked.md`.

## Step 2 - Run the pre-merge gate

```bash
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

It must be green before proceeding.

## Step 2.5 - Re-validate design assumptions

Before kicking off the validation gate, walk the design's
`phases.design.assumptions` list and record what actually happened.
Each design assumption maps to one of three outcomes:

| Outcome     | Meaning                                                    |
|-------------|------------------------------------------------------------|
| `held`      | The assumption was true in the implementation. Cite proof. |
| `violated`  | The assumption was false. Explain what we did instead.     |
| `untested`  | The assumption was not exercised by this slice. Note why.  |

`evidence` should be concrete: a test name (`src/workflow.rs::tests::
rejects_oversized_payload`), a file:line for a runtime check, or
"observed in prod via <metric>" for assumptions verified post-deploy.

If any assumption flipped to `violated`, the implementation diverged
from the design. That is allowed - but the synthesis reviewer must be
told, because it changes the size / scope of what to validate. Record
the divergence in `details`; the synthesis prompt will pick it up.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "implement",
  action: "assumptions_revalidated",
  details: "<n> assumptions reviewed. held: <n>, violated: <n>, untested: <n>. <one-line summary of violations>.",
  phase_updates: {
    assumptions_revalidated: true,
    assumption_outcomes: [
      { text: "...", outcome: "held", evidence: "src/<module>.rs::tests::test_name" }
    ]
  }
})
```

If `phases.design.assumptions` is empty (or this is a
specification-type task with no design phase), set
`assumptions_revalidated: true` with `assumption_outcomes: []` and
explain in `details`.

## Step 3 - Record implementation complete

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "implement",
  action: "implementation_complete",
  details: "All sub-tasks landed. fmt+clippy+test green. <n> commits.",
  phase_updates: { status: "validating" }
})
```

Note: `status` is `validating`, NOT `complete`. The phase only completes
after Step 4 produces all three review files and the synthesis verdict
permits it (Step 4.5). Setting `status: complete` here would let an agent
open a PR while skipping the validation gate - that is the bug this
ordering prevents. Do NOT call `transition_phase` until Step 4.6 passes.

## Step 4 - Two-reviewer validation + synthesis gate (MANDATORY)

This is the gcm equivalent of the mentis `review` phase. It is a
**bounded** gate:

1. Run Codex and Gemini concurrently as independent raw reviewers.
2. Save both raw reports.
3. Run a third model to synthesize the two reports, classify scope, and
   decide what (if anything) must be fixed.
4. Apply at most **one** synthesis-approved fix iteration.
5. If the synthesis recommends a pivot or fundamental rework, stop and ask
   the user instead of auto-fixing.

The roster is intentionally asymmetric: design-review uses Gemini + Ollama + Claude
fallback during iteration, while `pre-pr-validation` uses Codex + Gemini for final PR
decisions.

Never loop indefinitely on reviewer suggestions. Raw reviewer reports are
inputs; only the synthesis report drives fixes.

### 4.1 Run the validation gate via `lok`

The codex+gemini+synthesis pipeline lives in
`.lok/workflows/pre-pr-validation.toml`. Do **not** reinvent it inline.
The LLM running this phase MUST invoke `lok` and let the workflow engine
manage prompt assembly, parallel reviewer dispatch, output validation,
and review-file writes.

Anti-pattern (do not do this): writing a `/tmp/run_validation.sh` that
shells out to `codex exec` and `gemini` directly. That bypasses the
workflow's output validators, fallback logic, and synthesis step, and it
hardcodes models that drift over time. Always go through `lok`.

### 4.2 Invoke the `pre-pr-validation` workflow

```bash
# arg.1 = design doc path, arg.2 = plan file path, arg.3 = Linear task ID
lok workflow run pre-pr-validation docs/design-docs/clo-XX-design.md docs/plans/clo-XX-plan.md CLO-XX
```

Optional environment overrides:

| Variable | Default | Purpose |
|---|---|---|
| `CODEX_MODEL` | `gpt-5.5` | Codex model used for the codex reviewer |
| `GEMINI_MODEL` | `gemini-3.5-flash` | Primary Gemini model |
| `GEMINI_FALLBACK_MODEL` | `gemini-2.5-pro` | Used if the primary returns empty |

The workflow writes (and the rest of this phase reads):

- `docs/reviews/clo-XX-codex-validation.md`
- `docs/reviews/clo-XX-gemini-validation.md`
- `docs/reviews/clo-XX-validation-synthesis.md`

If both external reviewers fail, the workflow runs a Claude fallback and
writes `docs/reviews/clo-XX-claude-fallback-validation.md`. The synthesis
step still runs and produces the binding verdict.

If `lok workflow run` exits non-zero or any of the three required files
is missing/empty, treat the gate as failed: do **not** transition phases.
Instead, mark the workflow blocked and stop:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "implement",
  action: "validation_gate_failed",
  details: "lok pre-pr-validation exited non-zero (or required review files missing). See docs/reviews/clo-XX-*.md and lok output.",
  workflow_updates: { status: "blocked" },
});
```

Then investigate the failure (re-run with `--verbose`, inspect the
reviewer report files), fix the root cause, and re-run the workflow.
Only after a clean run produces all three review files may the workflow
status return to `active` and the phase resume. Never patch around the
failure by hand-writing review files. Do **not** call `transition_phase`
while the gate is failed - the state machine in
`extensions/orchestrate/index.ts` will reject the transition anyway
because `PHASE_CONFIG.implement` requires the synthesis report fields
and the `codex_validation_complete` history event.

### 4.3 Synthesis verdict

The synthesis report (`docs/reviews/clo-XX-validation-synthesis.md`)
ends with a `## Verdict` line that is exactly one of:

```
PASS | PASS_WITH_NOTES | FAIL
```

Legacy synonyms (`approve` / `approve_with_changes` / `pivot` / `rework`)
remain accepted by `PHASE_CONFIG` for older workflow YAMLs. The synthesis
prompt and verdict rules live in the workflow file. Do not duplicate them
here.

### 4.4 Act on synthesis verdict

| Synthesis verdict | Action |
|---|---|
| `PASS` (legacy: `approve`) | Proceed to Step 5. |
| `PASS_WITH_NOTES` (legacy: `approve_with_changes`) | Apply only `Must Fix Before PR` items, once. Run `cargo fmt --check && cargo clippy -- -D warnings && cargo test`, commit fixes, update synthesis with `## Re-validation`, then proceed. Do not rerun a new unbounded review loop. |
| `FAIL` (legacy: `pivot` / `rework`) | Stop. Set workflow blocked/pending human action and ask the user with synthesis recommendations. Do not transition to PR. |

Maximum validation fix iterations: **1**. If fixes reveal more issues,
record them in the synthesis report and ask the user whether to continue.

### 4.5 Record validation result

Only set `status: complete` when the synthesis verdict is `PASS` or
`PASS_WITH_NOTES` (legacy: `approve` / `approve_with_changes`) AND (for
`PASS_WITH_NOTES`) the single fix iteration has been applied and the
pre-merge gate is green again. For `FAIL` (legacy: `pivot` / `rework`),
leave `status: validating` and stop - the phase is not complete; jump to
Step 4.4's escalation path.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "implement",
  action: "codex_validation_complete",
  details: "Codex: <verdict>. Gemini: <verdict>. Synthesis: <verdict>. <fixes> applied.",
  phase_updates: {
    status: "complete",   // ONLY for PASS / PASS_WITH_NOTES (after fix)
    codex_validated: true,
    codex_verdict: "<PASS|PASS_WITH_NOTES|FAIL>",
    codex_report: "docs/reviews/clo-XX-codex-validation.md",
    gemini_validation_report: "docs/reviews/clo-XX-gemini-validation.md",
    validation_synthesis_report: "docs/reviews/clo-XX-validation-synthesis.md",
    validation_synthesis_verdict: "<PASS|PASS_WITH_NOTES|FAIL>",
    validation_fix_iteration_count: 0 | 1
  },
  token_usage: [
    { provider: "codex", model: "gpt-5.5", prompt_tokens: <p>, completion_tokens: <c>, task_label: "pre-pr-validation-codex" },
    { provider: "gemini", model: "gemini-3.5-flash", prompt_tokens: <p>, completion_tokens: <c>, task_label: "pre-pr-validation-gemini" },
    { provider: "claude", model: "claude-opus-4", prompt_tokens: <p>, completion_tokens: <c>, task_label: "pre-pr-validation-synthesis" }
  ]
})
```

If `validation_fix_iteration_count` was `1`, append a second `token_usage`
record for the fix-iteration dispatches via a follow-up
`update_workflow_state` call (the array is append-only across calls).

`codex_verdict` remains for backward compatibility. Use the synthesis
verdict as the decision source for PR transition.

### 4.6 Pre-transition checklist (MANDATORY)

Before calling `transition_phase` in Step 5, every item below MUST hold.
If any check fails, the validation gate has not passed - either return
to Step 4.4 (apply the single permitted fix) or stop and escalate to the
user. Do NOT open a PR, do NOT call `transition_phase`, do NOT mark the
phase complete.

Run the file-existence check verbatim:

```bash
TASK=clo-XX
for f in \
  docs/reviews/${TASK}-codex-validation.md \
  docs/reviews/${TASK}-gemini-validation.md \
  docs/reviews/${TASK}-validation-synthesis.md
do
  if [ ! -s "$f" ]; then
    echo "GATE FAIL: missing or empty $f"
    exit 1
  fi
done
echo "GATE OK: all three validation reports present"
```

Then verify each item by reading the workflow YAML and the synthesis
report:

- [ ] `phases.implement.status == "complete"` (not `validating`,
      `in_progress`, or unset).
- [ ] `phases.implement.codex_validated == true`.
- [ ] `phases.implement.codex_report` points to an existing,
      non-empty file with a final `## Verdict` section.
- [ ] `phases.implement.gemini_validation_report` points to an existing,
      non-empty file with a final `## Verdict` section.
- [ ] `phases.implement.validation_synthesis_report` points to an
      existing, non-empty file.
- [ ] `phases.implement.validation_synthesis_verdict` is `PASS` or
      `PASS_WITH_NOTES` (legacy: `approve` / `approve_with_changes`).
      Anything else is a stop.
- [ ] If verdict is `PASS_WITH_NOTES` (legacy: `approve_with_changes`),
      `phases.implement.validation_fix_iteration_count == 1` AND every
      "Must Fix Before PR" item from the synthesis report is reflected
      in the diff (re-read the synthesis to confirm).
- [ ] `cargo fmt --check && cargo clippy -- -D warnings && cargo test` is green on the current HEAD (re-run if any commits
      landed since the last green run).
- [ ] History contains both `implementation_complete` and
      `codex_validation_complete` events.

If any synthesis "Must Fix" item is unaddressed, the gate fails -
returning to it later as PR-review feedback is not acceptable. Codex and
Gemini are pre-PR reviewers; the human and bot reviewers in Step 3.5 of
`pr.md` are not a substitute.

## Step 5 - Transition to PR

Only after every Step 4.6 box is checked:

```ts
transition_phase({
  task_id: "CLO-XX",
  from_phase: "implement",
  to_phase: "pr"
})
```

## Notes

- The validation gate is non-negotiable for development tasks. For
  specification tasks the gate is recommended but may be skipped if the
  spec author opts out (record the decision in `details`).
- If validation surfaces a fundamental design issue, do not paper over
  it - return to `design` via user confirmation.
