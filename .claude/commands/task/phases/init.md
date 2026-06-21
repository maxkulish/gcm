# Phase: Initialize / Resume Workflow

**Purpose**: Parse arguments, initialize new workflows or resume existing ones, classify task type, and sync project aggregation files.

**Called by**: `/task:orchestrate` dispatcher

---

## Step 1: Parse Arguments

1. **Extract task number** from arguments (e.g., `CLO-9` or `clo-9`)
2. **Check for flags**:
   - `--status`: Display current state and exit (load `phases/status.md`)
   - `--ops`: Force operational task type (skip design/plan phases)
   - `--spec`: Force specification task type (use /spec instead of full design doc)
   - `--skip-discovery`: Skip discovery phase for development tasks (go straight to design)
3. **If no task provided**: Ask user interactively

---

## Step 2: Initialize or Resume Workflow

### Check for Existing Workflow State

```bash
# Look for workflow state file
ls docs/status/clo-XX-workflow.yaml 2>/dev/null
```

**If workflow state exists**:
1. Read `docs/status/clo-XX-workflow.yaml`
2. Display current state summary
3. Ask: "Resume from [current phase]? (yes/restart/cancel)"
   - **yes**: Continue from current phase
   - **restart**: Reset workflow to beginning
   - **cancel**: Exit

**If workflow state does NOT exist**:
1. Create new workflow state file
2. Fetch Linear task details using `mcp__linear-server__get_issue`
3. **Classify task type**: Proceed to Step 2.3
4. Initialize phases based on task type
5. Set initial phase and status
6. **Sync project files**: Proceed to Step 2.5

---

## Step 2.3: Classify Task Type & Recommend (New Workflow Only)

This step produces an **evidence-backed recommendation** for the task type, presents it for confirmation with the recommended option pre-selected, and records the rationale in the workflow YAML. The hardest call is the specification-vs-development boundary, so that is where the analytical effort goes; operational is detected cheaply from labels and keywords.

### 2.3.0 - Flag override

- **If `--ops` flag provided**: Set `task_type: operational`, `classification.method: flag`. Skip the probe, proceed to 2.3.5.
- **If `--spec` flag provided**: Set `task_type: specification`, `classification.method: flag`. Skip the probe, proceed to 2.3.5.
- If a flag contradicts the cheap operational signals (2.3.1), print a one-line heads-up but honor the flag (explicit user intent wins).

### 2.3.1 - Operational gate (cheap, first)

Scan Linear labels and title for operational indicators:
- **Labels**: `ops`, `maintenance`, `admin`, `devops`, `bug`, `hotfix`
- **Title keywords**: `restore`, `backup`, `migrate`, `configure`, `setup`, `investigate`, `cleanup`

If **2+ indicators** are present:
- Set `classification.recommended: operational`, `classification.method: operational_gate`, `classification.confidence: high`
- Skip the codebase probe, go to 2.3.4 (still presented for confirmation)

Otherwise, proceed to 2.3.2.

### 2.3.2 - Evidence gathering (the check)

Runs for the specification-vs-development boundary.

**Metadata** (from the Linear issue already fetched): title, description, labels, comments, subtask count, estimate.

**Codebase probe** (bounded to stay cheap):
1. Extract up to ~5 candidate component or symbol names from the description.
2. Use Grep/Glob to **locate** them - locate, do not read full files.
3. Count distinct top-level modules implicated -> blast radius.
4. Search for similar existing code -> prior art.
5. Check `docs/adrs/` and `docs/design-docs/` for related decisions.
6. Flag architecture-decision triggers: new dependency, new data model or migration, new public interface, new process boundary, state-ownership or concurrency change.

Fill the evidence table. Each row records `lean: spec|dev|neutral` and one line of evidence drawn from the probe (record `neutral` when the evidence is inconclusive):

| Dimension | Leans SPEC | Leans DEVELOPMENT |
|---|---|---|
| Blast radius | single module / few files | cross-module / many files |
| Architecture novelty | follows an existing pattern | new data model, dependency, interface, process boundary, or state/concurrency change |
| Prior art | similar existing code is a template | greenfield, no template |
| Requirement clarity | acceptance criteria derivable now | open questions / trade-offs remain |
| Scope signal | S/M estimate | L/XL estimate |

### 2.3.3 - Verdict and confidence

Make a **holistic verdict** over the table (not a weighted sum). Derive confidence from row agreement:
- 4-5 rows aligned -> `high`
- 3/2 split -> `medium`
- mostly neutral or even -> `low`

A vague ticket where the probe finds little produces neutral rows and therefore low confidence, which correctly signals the task needs the discussion a full cycle provides.

Set `classification.recommended` to the verdict and `classification.method: probe`.

### 2.3.4 - Present and confirm

Display the recommendation with the recommended option pre-selected (Enter accepts):

```
TASK CLASSIFICATION - CLO-XX: [title]

Recommendation:  [TYPE]   confidence: [HIGH|MEDIUM|LOW] ([N]/5 signals aligned)

Evidence:
  Blast radius          [lean]   [note]
  Architecture novelty  [lean]   [note]
  Prior art             [lean]   [note]
  Requirement clarity   [lean]   [note]
  Scope signal          [lean]   [note]

Rationale: [one-line synthesis]

  1. [development]    full cycle (discovery -> design -> plan -> implement -> PR)
                      For: new features with architecture decisions, cross-module changes (L scope)
  2. [specification]  lean (spec -> implement -> PR)
                      For: well-scoped features, clear requirements, single-module changes (S/M scope)
  3. [operational]    streamlined (execute -> document -> PR if needed)
                      For: troubleshooting, configuration, admin tasks, investigations
  4. [pause]          save state, continue later

(The RECOMMENDED marker appears on the recommended option.)
Choice [Enter = recommended]:
```

- **Enter** or selecting the recommendation -> accept.
- **A different number** -> override; record the divergence in `classification.chosen`.
- **pause** -> save state and exit with resume instructions.

Set `classification.chosen` to the user's selection (operational gate short-circuits here too: present, confirm, set `chosen`).

### 2.3.5 - Record and initialize

Write the classification block to the workflow YAML:

```yaml
classification:
  recommended: <type>
  chosen: <type>               # may differ from recommended
  confidence: <high|medium|low>
  method: <flag|operational_gate|probe>
  evidence:                    # omit when method is flag or operational_gate
    blast_radius:         { lean: <spec|dev|neutral>, note: "<evidence>" }
    architecture_novelty: { lean: <spec|dev|neutral>, note: "<evidence>" }
    prior_art:            { lean: <spec|dev|neutral>, note: "<evidence>" }
    requirement_clarity:  { lean: <spec|dev|neutral>, note: "<evidence>" }
    scope_signal:         { lean: <spec|dev|neutral>, note: "<evidence>" }
  rationale: "<one-line synthesis>"
```

Add history entry: `task_classified` (details: recommended, chosen, confidence).

Then initialize phases for the chosen `task_type`:

   **For Development tasks**:
   ```yaml
   task_type: development
   workflow:
     current_phase: discovery    # or "design" if --skip-discovery
     status: awaiting_input
   phases:
     discovery: { status: pending }   # or { status: skipped, skip_reason: "--skip-discovery flag" }
     design: { status: pending }
     plan: { status: pending }
     implement: { status: pending }
     pr: { status: pending }
     complete: { status: pending }
   ```

   If `--skip-discovery` flag is set:
   - Set `workflow.current_phase: design`
   - Set `phases.discovery.status: skipped`
   - Set `phases.discovery.skip_reason: "--skip-discovery flag"`
   - Set `phases.discovery.approved: true`

   **For Specification tasks**:
   ```yaml
   task_type: specification
   workflow:
     current_phase: spec
     status: awaiting_input
   phases:
     discovery: { status: skipped, skip_reason: "Specification task", approved: true }
     design: { status: skipped, reason: "Specification task - using /spec instead" }
     spec: { status: pending, spec_file: null, approved: false }
     implement: { status: pending }
     pr: { status: pending }
     complete: { status: pending }
   ```

   **For Operational tasks**:
   ```yaml
   task_type: operational
   workflow:
     current_phase: execute
     status: in_progress
   phases:
     discovery: { status: skipped, skip_reason: "Operational task", approved: true }
     design: { status: skipped, reason: "Operational task" }
     plan: { status: skipped, reason: "Operational task" }
     execute: { status: pending }
     document: { status: pending }
     pr: { status: pending, required: false }
     complete: { status: pending }
   ```

---

## Step 2.5: Sync Project Aggregation Files (New Workflow Only)

**IMPORTANT**: This step only runs when starting a NEW workflow (not resuming).

1. **Invoke**: `/project:sync CLO-XX --start`

2. **This validates**:
   - WIP limit (max 3 active tasks in PROJECT.md)
   - Task is not blocked (check DEPENDENCIES.md)

3. **If validation fails**:
   - `/project:sync` will display the issue and options
   - User must resolve before proceeding
   - Workflow enters `blocked` state until resolved

4. **If validation passes**:
   - PROJECT.md: Task added to "Active Work"
   - ROADMAP.md: Task status changed to "In Progress"
   - DEPENDENCIES.md: Task removed from "Unblocked & Ready"
   - Add history entry: `project_sync_start`

---

## Return to Dispatcher

After initialization/resume completes, return control to the dispatcher with `current_phase` set. The dispatcher will load the appropriate phase file.
