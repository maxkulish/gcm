# Design: Evidence-Backed Task Classification for /task:orchestrate

**Date**: 2026-06-20
**Status**: Approved (pending implementation plan)
**Affected files**: `.claude/commands/task/phases/init.md` (Step 2.3 rewrite), `.claude/commands/task/orchestrate.md` (history table + philosophy note)

---

## Problem

`/task:orchestrate` begins by classifying a task as one of three workflows: `development` (discovery -> design -> plan -> implement -> PR), `specification` (spec -> implement -> PR), or `operational` (execute -> document -> PR). The classification decides how much process the task carries, so getting it wrong is expensive in both directions: a misrouted spec task drags a small change through full design discovery, and a misrouted development task starts implementing before the architecture is settled.

Today the relevant logic lives in `init.md` Step 2.3. It lists "indicators found" and then presents a flat three-way menu and asks the user to choose. The model gathers no evidence and offers no recommendation. The user carries the full decision every time, including for tasks where the right answer is obvious from the ticket and the codebase.

The hardest call in practice is the boundary between `specification` and `development`. Operational tasks are usually clear from labels and keywords. So the analytical effort belongs on the spec-vs-development fork.

## Goal

Replace the blind ask with an evidence-backed recommendation. The model checks the ticket and the codebase, produces a recommendation with a confidence level and a per-signal rationale, and pre-selects the recommended workflow. The user confirms with a single keystroke or overrides. Every recommendation is recorded in the workflow YAML so the call is auditable later.

## Decisions

These were settled during brainstorming:

- **Check depth**: metadata plus a bounded codebase probe. The recommendation reads the Linear ticket and also inspects the repository to estimate blast radius and detect architecture decisions.
- **Autonomy**: always present and confirm. The model never auto-skips the checkpoint. Confidence is still computed because it shapes how the recommendation reads and future-proofs an optional "auto-proceed on high confidence" mode.
- **Scope**: all three types stay, but the spec-vs-development boundary gets the analytical work. Operational stays label-driven and cheap.
- **Mechanism**: hybrid. A deterministic check fills a structured evidence table; the model makes a holistic verdict over that table; confidence derives from how well the rows agree. This keeps the rigor of a real check without the false precision of a weighted numeric score.

## The signals

Five dimensions distinguish a specification task from a development task. Each is gathered deterministically and each row of the evidence table records a `lean` and one line of actual evidence.

| Dimension | Leans SPEC when | Leans DEVELOPMENT when |
|---|---|---|
| Blast radius | single module / few files implicated | cross-module / many files implicated |
| Architecture novelty | follows an existing pattern in the repo | new data model, new dependency, new public interface, new process boundary, or state-ownership/concurrency change |
| Prior art in repo | similar existing code can serve as a template | greenfield, no template to follow |
| Requirement clarity | acceptance criteria are derivable now | open questions, trade-offs, or "should we" decisions remain |
| Scope signal | S/M estimate | L/XL estimate |

A row whose evidence is inconclusive records `lean: neutral`.

## The routine (new Step 2.3)

The rewrite turns Step 2.3 into a five-part routine.

### 2.3.0 - Flag override

`--ops` and `--spec` set the task type directly and skip the probe. Explicit user intent wins. Record `classification.method: flag`. If the flag contradicts the cheap operational signals, print a one-line heads-up but honor the flag.

### 2.3.1 - Operational gate (cheap, first)

Scan labels and title keywords (`ops`, `maintenance`, `admin`, `devops`, `bug`, `hotfix`, `restore`, `backup`, `migrate`, `configure`, `setup`, `investigate`, `cleanup`). Two or more indicators recommend `operational` and short-circuit before the probe runs. Record `classification.method: operational_gate`. The recommendation is still presented for confirmation per the "always confirm" decision.

### 2.3.2 - Evidence gathering (the check)

Runs only for the spec-vs-development boundary.

Metadata: title, description, labels, comments, subtask count, estimate.

Codebase probe, bounded to stay cheap:

- Extract up to roughly five candidate component or symbol names from the description.
- `grep` / `glob` to locate them. Locate, do not read full files.
- Count distinct top-level modules implicated to estimate blast radius.
- Search for similar existing code to assess prior art.
- Check `docs/adrs/` and `docs/design-docs/` for related decisions.
- Flag architecture-decision triggers: new dependency, new data model or migration, new public interface, new process boundary, state-ownership or concurrency change.

Fill the five-row evidence table. Each row gets `lean: spec|dev|neutral` plus one line of evidence drawn from the probe.

### 2.3.3 - Verdict and confidence

Make a holistic verdict over the table rather than a weighted sum. Derive confidence from row agreement:

- 4 or 5 rows aligned -> high
- 3/2 split -> medium
- mostly neutral or even -> low

A vague ticket where the probe finds little produces neutral rows and therefore low confidence, which correctly signals that the task needs the discussion a full cycle provides.

### 2.3.4 - Present and confirm

Replaces the old flat menu. The recommended option is pre-selected and Enter accepts it.

```
TASK CLASSIFICATION - CLO-XX: [title]

Recommendation:  [SPECIFICATION]   confidence: HIGH (4/5 signals aligned)

Evidence:
  Blast radius          spec   only internal/tracer touched
  Architecture novelty  spec   follows existing Groq client pattern
  Prior art             spec   internal/ai/groq.go is a usable template
  Requirement clarity   dev    one open question on grouping heuristic
  Scope signal          spec   estimate: M

Rationale: 4/5 favor a lean spec; the single open question is resolvable in spec Q&A.

  1. [development]    full cycle (discovery -> design -> plan -> implement -> PR)
  2. [specification]  RECOMMENDED  lean (spec -> implement -> PR)
  3. [operational]    streamlined (execute -> document -> PR)
  4. [pause]

Choice [Enter = recommended]:
```

### 2.3.5 - Record and initialize

Write the `classification` block to the workflow YAML, then run the existing per-type phase initialization unchanged.

## YAML addition

```yaml
classification:
  recommended: specification
  chosen: specification        # may differ from recommended
  confidence: high             # high|medium|low
  method: probe                # flag|operational_gate|probe
  evidence:
    blast_radius:         { lean: spec, note: "only internal/tracer touched" }
    architecture_novelty: { lean: spec, note: "follows existing Groq client pattern" }
    prior_art:            { lean: spec, note: "internal/ai/groq.go is a template" }
    requirement_clarity:  { lean: dev,  note: "open question on grouping heuristic" }
    scope_signal:         { lean: spec, note: "estimate: M" }
  rationale: "4/5 favor a lean spec; open question resolvable in spec Q&A."
```

## orchestrate.md edits

- Add a `task_classified` history action type (details: recommended, chosen, confidence).
- Add one line under Philosophy noting that classification is now an evidence-backed recommendation rather than a blind ask.

## Safety net

`spec.md` already offers `[upgrade] -> development` when a spec turns out more complex than expected, so a wrong "spec" recommendation self-corrects mid-flow. The mirror case (development downgraded to spec) is rare and adds process surface, so it is intentionally out of scope (YAGNI).

## Out of scope

- A development-to-spec downgrade hook.
- Auto-proceed on high confidence (the YAML records confidence so this can be added later without rework).
- Changes to any phase other than init, beyond the two small orchestrate.md edits.
