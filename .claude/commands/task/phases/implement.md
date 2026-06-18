# Phase: Implement

**Purpose**: Execute the implementation plan phase by phase, tracking commits and pushing to remote. Run external model validation before transitioning to PR.

**Entry conditions**: `current_phase: implement`

---

## Status: pending or in_progress

1. Update state: `phases.implement.status: in_progress`
2. **Invoke**: `/plan:implement CLO-XX`

3. After each phase completion within `/plan:implement`:
   - Update workflow state:
     - `phases.implement.last_phase_completed: [phase name]`
     - Add commit SHA to `phases.implement.commits[]`
   - **Push to remote**:
     ```bash
     git push origin feat/clo-XX-short-desc
     ```
   - Add history entry: `phase_completed` with details of phase name
   - Add history entry: `pushed_to_remote`

4. When `/plan:implement` reaches 100%:
   - Add history entry: `implementation_complete`
   - **Continue to Validation Gate** (Step 5)

---

## Step 5: Codex + Gemini Validation Gate

**After implementation is complete, before creating a PR**, run external model validation to catch issues Claude may have blind spots for.

### Build Unified Validation Prompt

```
You are a senior code reviewer. Review all changes on this branch against the design
document and implementation plan.

FILES TO READ:
1. The design document: [path from phases.design.design_doc]
2. The implementation plan: [path from phases.plan.plan_file]
3. Run: git diff main...HEAD (to see all changes)
4. Read any new or significantly modified source files

CHECK FOR:
1. CORRECTNESS: Do the changes implement what the design doc specifies?
2. COMPLETENESS: Are all acceptance criteria from the design doc covered?
3. REGRESSIONS: Could any changes break existing functionality?
4. CODE QUALITY: Clean interfaces, proper error handling, no dead code
5. SECURITY: No hardcoded secrets, proper input validation, safe FFI usage

OUTPUT FORMAT:
## Verdict: [PASS | PASS_WITH_NOTES | FAIL]

## Findings
[List each finding with severity: CRITICAL / HIGH / MEDIUM / LOW]

## Missing Items
[Any acceptance criteria not yet implemented]

## Recommendations
[Specific actionable improvements]
```

### Run Validation in Parallel

```bash
# Codex validation (background) - 10 minute timeout
timeout 600 codex exec -m gpt-5.4 \
  -c reasoning.effort='"high"' \
  -s read-only \
  -o docs/reviews/clo-XX-codex-validation.md \
  "[VALIDATION_PROMPT]" &

# Gemini validation (background) - 5 minute timeout
(
  timeout 300 gemini --model gemini-3.1-pro-preview -y --sandbox \
    --include-directories docs,src \
    -p "[VALIDATION_PROMPT]" -o text \
    > docs/reviews/clo-XX-gemini-validation.md 2>&1
) &

# Wait for both
wait
```

### Display Results

```
VALIDATION GATE RESULTS (CLO-XX)
=================================

Codex (GPT-5.4):
  Verdict: [PASS | PASS_WITH_NOTES | FAIL]
  Report: docs/reviews/clo-XX-codex-validation.md
  Key Findings: [top 3 findings]

Gemini (3.1 Pro):
  Verdict: [PASS | PASS_WITH_NOTES | FAIL]
  Report: docs/reviews/clo-XX-gemini-validation.md
  Key Findings: [top 3 findings]

Options:
1. [proceed]  - Continue to PR creation
2. [fix]      - Address findings before PR (recommended if FAIL)
3. [override] - Skip validation and proceed (not recommended)
4. [pause]    - Pause workflow

Your choice:
```

### Decision Handling

- **If proceed**: Update state and continue to PR phase
- **If fix**: Address findings, re-commit, re-run validation
- **If override**: Log override in history, proceed with warning
- **If pause**: Save state, exit

### Fallback

- If Codex is unavailable: Warn and run Gemini only
- If Gemini is unavailable: Warn and run Codex only
- If both unavailable: Warn and let user decide (proceed or pause)

### Update State

- `phases.implement.codex_validated: true`
- `phases.implement.codex_verdict: [verdict]`
- `phases.implement.codex_report: docs/reviews/clo-XX-codex-validation.md`
- `phases.implement.gemini_validation_report: docs/reviews/clo-XX-gemini-validation.md`
- Add history entry: `codex_validation_complete`

### Transition to PR

- `phases.implement.status: complete`
- `workflow.current_phase: pr`
- `workflow.status: in_progress`
- **Continue to PR phase**

---

## YAML Checkpoint (Required before transition)

Before signaling completion to the dispatcher, verify:
- `phases.implement.status: complete`
- `phases.implement.commits` is non-empty
- History contains `implementation_complete`
- `phases.implement.codex_validated` is set (true if ran, false if skipped/unavailable)
