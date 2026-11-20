---
context: fork
allowed-tools:
  - Read
  - Glob
  - Bash(ls:*)
  - Bash(mkdir:*)
  - Bash(lok:*)
  - Write
---

# /design-doc:review - AI Review of Design Documents

**Purpose**: Automatically review design documents using multiple AI models (Gemini + Codex) with Claude as a guaranteed fallback. Identifies architectural issues, ADR compliance, blind spots, and provides actionable feedback before human review.

**Usage**:
- `/design-doc:review CLO-XX` - Review with both models + Claude fallback if needed
- `/design-doc:review CLO-XX --model gemini` - Gemini only
- `/design-doc:review CLO-XX --model codex` - Codex only
- `/design-doc:review CLO-XX --persona all` - Run all domain-specific persona reviews
- `/design-doc:review` - Interactive mode

**Resilience guarantees**:
- Pre-flight health check skips unreachable models immediately
- A shared review guard (`.lok/lib/review-guard.sh`) turns a non-zero exit, a timeout, empty or short output, help or usage text, and a report without a verdict into one `REVIEW_FAILED: <reviewer> <model>: <reason>` line
- Claude fallback reviewer (read-only tools) activates when both external models fail
- Synthesis adapts format: multi-review (Agreement/Disagreement) vs single-review (Key Findings)

---

## Key Differences: Gemini vs Codex

| Aspect | Gemini (opencode, Vertex) | Codex |
|--------|---------------------------|-------|
| **File Reading** | opencode read, grep, glob and list tools | Shell commands in a read-only sandbox (`cat`, `find`, `head`) |
| **Permissions** | `OPENCODE_CONFIG_CONTENT` denies every tool except read, grep, glob and list | `-s read-only` (no writes allowed) |
| **Approach** | Model reads files itself | Agent reads files via shell |
| **Command** | `opencode run "..." --model "google-vertex/${GEMINI_MODEL:-gemini-4-hydrogen#high}" --agent plan --auto --standalone` | `codex exec -m "${CODEX_MODEL:-gpt-6.2-sol}" -c model_reasoning_effort='"high"' -s read-only --ephemeral -o <file> "..."` |

---

## When to Use

This command is typically invoked:
1. Automatically by `/task:orchestrate` after `/design-doc:create` completes
2. Manually when requesting an AI review of any design document

---

## Unified Review Prompt

**Both Gemini and Codex receive the same review prompt.** Only the file-reading instructions differ slightly.

```
You are a senior software architect reviewing a design document.

TASK: Review the design document at: docs/design-docs/[DESIGN_DOC_FILENAME]

Read these files to gather context:
1. docs/design-docs/[DESIGN_DOC_FILENAME] — The design document to review
2. docs/arch/ — Architecture documents (read all .md files in this directory)
3. docs/adrs/ — Architecture Decision Records (60+ files). List filenames first, then read ONLY the ADRs relevant to this design document's topic (typically 5-10 most relevant)
4. docs/ROADMAP.md — Current project phase and task status
5. docs/DEPENDENCIES.md — Task dependency graph
6. docs/PROJECT.md — Active work and blockers

If the design document references specific source files, read those too for validation.

PROJECT CONTEXT:
- This is a Rust CLI tool for multi-LLM orchestration
- Linear workspace: cloud-ai
- Issue prefix: CLO

REVIEW CRITERIA:

1. COMPLETENESS (check all sections are present and meaningful)
   - Summary: Clear problem statement and solution
   - Background: Sufficient context
   - Architecture: Component overview
   - Detailed Design: Implementation approach
   - Implementation Plan: Phased approach with clear tasks
   - Acceptance Criteria: Testable success metrics

2. ARCHITECTURE QUALITY
   - Appropriate design patterns
   - Clear separation of concerns
   - Scalability considerations
   - Error handling strategy

3. ADR COMPLIANCE
   - List all ADR filenames in docs/adrs/ to understand the full decision landscape
   - Read the ADRs most relevant to this design document's topic (typically 5-10)
   - For each relevant ADR, check if the design document follows or contradicts the decisions
   - Flag any violations or deviations from established ADRs
   - Note if the design document introduces patterns that should become a new ADR

4. CODE QUALITY
   - Clean interfaces
   - Proper abstractions
   - Testability

5. SECURITY POSTURE
   - Input validation
   - Authentication/Authorization (if applicable)
   - No hardcoded secrets

6. OPERATIONAL READINESS
   - Logging and monitoring considered
   - Error recovery addressed
   - Rollback plan exists

7. ARCHITECTURAL ALIGNMENT
   - Aligns with existing architecture (from docs/arch and docs/adrs)
   - Follows established patterns
   - Fits within current project phase and dependencies

8. BLIND SPOTS
   - What is NOT covered in the design document that should be?
   - What edge cases or failure modes are missing?
   - What assumptions are made but not stated?
   - What cross-cutting concerns (logging, metrics, error handling, accessibility) are overlooked?
   - What integration points with existing code might cause unexpected issues?

OUTPUT FORMAT:

## 1. Completeness Check
[List sections present/missing with brief assessment]

## 2. Architecture Assessment
**Strengths**: [What's done well]
**Concerns**: [Issues to address]

## 3. ADR Compliance
[For each ADR in docs/arch/, state whether the design follows it]
**Violations**: [Any ADR violations found]
**New ADR Needed**: [If the design introduces patterns worthy of a new ADR]

## 4. Security Review
[Assessment of security posture]

## 5. Implementation Concerns
[Feedback on implementation plan]

## 6. Blind Spots
[What the design document misses or doesn't address]
- Missing edge cases
- Unstated assumptions
- Overlooked failure modes
- Missing cross-cutting concerns

## 7. Verdict
[One of: APPROVE | APPROVE_WITH_SUGGESTIONS | NEEDS_REVISION]

## 8. Actionable Feedback
[Prioritized list of specific, actionable items]
```

---

## Command Execution Instructions

### Step 1: Extract Task Number and Model Selection

1. **Parse arguments**:
   - Extract task number (e.g., `CLO-21` or `clo-21`)
   - Check for `--model gemini`, `--model codex`, or default to both
   - Check for `--persona [security|concurrency|backend-integration|all]` (optional, adds domain-specific reviews)

2. **If no task number provided**:
   - Ask: "Which design document do you want to review? (e.g., CLO-21)"
   - Wait for response

### Step 2: Locate Design Document

```bash
ls docs/design-docs/clo-XX-*.md
```

**If NOT found**:
```
ERROR: Design document not found
Expected: docs/design-docs/clo-XX-*.md
Create one first: /design-doc:create CLO-XX
```
Exit command.

**If found**: Note the full filename (e.g., `clo-21-websocket-handler.md`)

### Step 3: Verify Context Files

Verify these files exist (both models need them):
- `docs/design-docs/clo-XX-*.md` — The design document to review
- `docs/arch/` — Architecture documents directory
- `docs/adrs/` — Architecture Decision Records (60+ files)
- `docs/ROADMAP.md` — Project phases and task status
- `docs/DEPENDENCIES.md` — Task dependency graph
- `docs/PROJECT.md` — Active work and blockers

If any are missing, warn but proceed.

### Step 4: Create Reviews Directory

```bash
mkdir -p docs/reviews
```

### Step 5: Build the Review Prompt

Construct the unified prompt (from the template above) by replacing `[DESIGN_DOC_FILENAME]` with the actual filename. **Both models get the same prompt.**

### Step 6: Run AI reviews via lok

Run the lok design-review workflow. The workflow executes this pipeline:

1. **Health check** - probes Gemini through opencode on Vertex and checks the Codex CLI. An unreachable model is skipped immediately with a `REVIEW_FAILED` line.
2. **Gemini review** (up to 19 minutes) - runs behind the review guard.
3. **Codex review** (up to 19 minutes) - runs in parallel with Gemini, behind the review guard.
4. **Review status** - marks each report usable or failed with the shared usable-report check.
5. **Claude fallback** - runs only when no external report is usable. It reads the design doc and context files with read-only tools.
6. **Synthesis** - cross-references the usable reports only, with a reviewer status table.
7. **Write files** - stages every report, then renames them over the targets, the synthesis last. With zero usable reports the writer itself writes `NO_REVIEWS_AVAILABLE`; with an unusable synthesis it writes `SYNTHESIS_FAILED: <reason>`.

Run in background:
```bash
lok run .lok/workflows/design-review.toml \
  "docs/design-docs/[DESIGN_DOC_FILENAME]" \
  "clo-[XX]" \
  --dir . \
  --verbose
```

This produces:
- `docs/reviews/clo-[XX]-review-gemini.md` (usable Gemini review or REVIEW_FAILED)
- `docs/reviews/clo-[XX]-review-codex.md` (usable Codex review or REVIEW_FAILED)
- `docs/reviews/clo-[XX]-review-claude-fallback.md` (only if both external models failed)
- `docs/reviews/clo-[XX]-review-synthesis.md` (cross-referenced synthesis with reviewer status)

### Step 7: Save Review Outputs

**Gemini review**: `docs/reviews/clo-XX-review-gemini.md`
**Codex review**: `docs/reviews/clo-XX-review-codex.md`

**Format for each**:
```markdown
# Design Review: CLO-XX - [Title]

**Reviewed**: [Current Date YYYY-MM-DD]
**Reviewer**: [Gemini (google-vertex/<GEMINI_MODEL>) | Codex (<CODEX_MODEL>, effort high)]
**Design Document**: docs/design-docs/clo-XX-[description].md
**Review Duration**: [X seconds]

---

[AI REVIEW OUTPUT]

---

*This review was automatically generated. Human judgment should be applied when interpreting these suggestions.*
```

### Step 7.5: Run Persona Reviews (if --persona flag provided)

If `--persona` flag was provided, run additional domain-specific reviews. These run as Claude subagents, each reading the persona template and the design document.

**Available personas** (templates in `.claude/templates/review-personas/`):

| Persona | Template | Focus |
|---------|----------|-------|
| `security` | `review-personas/security.md` | API key handling, process execution, input validation |
| `concurrency` | `review-personas/concurrency.md` | Async safety, tokio patterns, race conditions |
| `backend-integration` | `review-personas/backend-integration.md` | Backend trait, error handling, timeout management |

**For each requested persona** (or all three if `--persona all`):

1. Read the persona template from `.claude/templates/review-personas/{persona}.md`
2. Build the persona review prompt by combining the template's review prompt with:
   - The design document path
   - Relevant source files (based on persona focus)
   - The persona's output format
3. Run as a subagent (Agent tool) with the persona prompt
4. Save output to `docs/reviews/clo-XX-review-{persona}.md`

**Output files**: `docs/reviews/clo-XX-review-security.md`, etc.

---

### Step 8: Keep the lok Synthesis and Summarize Personas

The lok run already wrote `docs/reviews/clo-XX-review-synthesis.md`, and its writer applied the synthesis check. Do not write, rewrite or append to that file. Its first line (`NO_REVIEWS_AVAILABLE`, `SYNTHESIS_FAILED: <reason>` or the synthesis heading) is the outcome that `phases/design.md` reads.

If personas ran, write their results to `docs/reviews/clo-XX-review-persona-summary.md`:

```markdown
## Persona Summary
| Persona | Verdict | Key Finding |
|---------|---------|-------------|
| [persona] | [verdict] | [1-line summary] |
```

Report the persona findings next to the lok synthesis in Step 9. They do not change the consolidated verdict of the lok synthesis.

### Step 9: Return Summary

```
========================================
DESIGN REVIEW COMPLETE
========================================

Design Document: docs/design-docs/clo-XX-[description].md

Reviews Generated:
  - docs/reviews/clo-XX-review-gemini.md (Xs)
  - docs/reviews/clo-XX-review-codex.md (Xs)
  - docs/reviews/clo-XX-review-synthesis.md
  [If personas ran:]
  - docs/reviews/clo-XX-review-security.md
  - docs/reviews/clo-XX-review-concurrency.md
  - docs/reviews/clo-XX-review-backend-integration.md
  - docs/reviews/clo-XX-review-persona-summary.md

Verdicts:
  - Gemini: [APPROVE | APPROVE_WITH_SUGGESTIONS | NEEDS_REVISION]
  - Codex: [APPROVE | APPROVE_WITH_SUGGESTIONS | NEEDS_REVISION]
  [If personas ran:]
  - Security: [SAFE | CONCERNS_HIGH | CONCERNS_MEDIUM]
  - Concurrency: [SAFE | CONCERNS_HIGH | CONCERNS_MEDIUM]
  - Backend Integration: [CORRECT | CONCERNS_HIGH | CONCERNS_MEDIUM]

Consensus: [APPROVE | NEEDS_REVISION | MIXED]

Key Findings:
1. [Top finding from synthesis]
2. [Second finding]
3. [Third finding]

Full reviews saved to: docs/reviews/
```

---

## Error Handling

**Check review results**

If lok exited non-zero, treat the run as "no AI reviews available" and read no report file. If lok also shows `REVIEW_PUBLISH_FAILED: see the run publish-failed file`, read `.lok/local/review-runs/<slug>/publish-failed` before any rerun: its line names the first report target that was not published (for example a directory at that path) and the reason. Later targets of that run were not attempted, and the next run deletes the file. Otherwise read `docs/reviews/clo-[XX]-review-synthesis.md`. A report has failed only when its first line starts with `REVIEW_FAILED:`. A synthesis whose first line is `SYNTHESIS_FAILED: <reason>` lists the usable individual reports to read instead. Otherwise the synthesis starts with a **Reviewer Status** table showing which models succeeded, failed, or were skipped.

**Interpreting the synthesis:**

| Scenario | Synthesis format | What happened |
|----------|-----------------|---------------|
| 2+ reviews valid | Multi Review (Agreement/Disagreement/Novel) | Normal multi-perspective synthesis |
| 1 review valid | Single Review (Key Findings) | One external model + possibly Claude fallback |
| Claude fallback only | Single Review, source = Claude | Both external models failed, fallback activated |
| `SYNTHESIS_FAILED` | First line, then the usable reports | At least one usable report, but the synthesis failed its check |
| `NO_REVIEWS_AVAILABLE` | Written by the shell step, not a model | All reviewers failed, the Claude fallback included |

**Failure diagnostics:**

The Reviewer Status table in the synthesis includes failure reasons inline:

```
## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini | REVIEW_FAILED | exit 124 (timeout) |
| Codex | REVIEW_FAILED | output too short (0 characters) |
| Claude (fallback) | OK | Produced full review |
```

For deeper diagnostics:
- Gemini stderr: `.lok/local/review-runs/clo-XX/gemini.stderr`
- Codex stderr: `.lok/local/review-runs/clo-XX/codex.stderr`

**The `NO_REVIEWS_AVAILABLE` case should be rare.** The Claude fallback reviewer has no external dependencies - it reads files directly. If even the fallback fails, it indicates a problem with the lok pipeline itself, not network issues.

---

## Configuration

### Default Models

| Provider | Model | Integration | Notes |
|----------|-------|-------------|-------|
| Gemini | `google-vertex/gemini-4-hydrogen#high` | opencode | Always under the `google-vertex/` provider |
| Codex | `gpt-6.2-sol`, effort high | Codex CLI | `codex exec -m MODEL -c model_reasoning_effort='"high"'` |

### Environment Variables

- `GEMINI_MODEL` - Override the Gemini model, without the provider (default: `gemini-4-hydrogen#high`, always run as `google-vertex/<model>`)
- `CODEX_MODEL` - Override the Codex model (default: `gpt-6.2-sol`)

---

## Output Files

| File | Purpose | When created |
|------|---------|--------------|
| `docs/reviews/clo-XX-review-gemini.md` | Gemini AI review | Always (may contain REVIEW_FAILED) |
| `docs/reviews/clo-XX-review-codex.md` | Codex AI review | Always (may contain REVIEW_FAILED) |
| `docs/reviews/clo-XX-review-claude-fallback.md` | Claude fallback review | Only when both external models failed |
| `docs/reviews/clo-XX-review-synthesis.md` | Cross-referenced synthesis with reviewer status | Always |

---

## Example: Running Both Reviews

```bash
DESIGN_DOC="clo-125-flat-model-list.md"
REVIEW_PROMPT="You are a senior software architect reviewing a design document.

TASK: Review the design document at: docs/design-docs/${DESIGN_DOC}

Read these files to gather context:
1. docs/design-docs/${DESIGN_DOC}
2. docs/arch/ (all .md files — Architecture Decision Records)
3. docs/ROADMAP.md
4. docs/DEPENDENCIES.md
5. docs/PROJECT.md

If the design document references specific source files, read those too.

PROJECT CONTEXT: Rust CLI tool for multi-LLM orchestration. Linear workspace: cloud-ai. Issue prefix: CLO.

[... full review criteria and output format ...]"

# Run Gemini (background); output to a temporary file, never straight to docs/reviews
. .lok/lib/review-guard.sh
T=$(mktemp -d)
OPENCODE_CONFIG_CONTENT="$REVIEW_OPENCODE_PERMISSION" timeout 1140 opencode run "$REVIEW_PROMPT" \
  --model "google-vertex/${GEMINI_MODEL:-gemini-4-hydrogen#high}" --agent plan --auto --standalone \
  </dev/null >"$T/gemini.out" 2>"$T/gemini.stderr" &
GP=$!

# Run Codex (background)
timeout 1140 codex exec -m "${CODEX_MODEL:-gpt-6.2-sol}" -c model_reasoning_effort='"high"' \
  -s read-only --ephemeral -o "$T/codex.out" "$REVIEW_PROMPT" \
  </dev/null >"$T/codex.stdout" 2>"$T/codex.stderr" &
CP=$!

# Wait for each by PID and keep each exit status
GRC=0; wait "$GP" || GRC=$?
CRC=0; wait "$CP" || CRC=$?
review_guard Gemini "google-vertex/${GEMINI_MODEL:-gemini-4-hydrogen#high}" "$GRC" "$T/gemini.out" "$T/gemini.stderr" review
review_guard Codex "${CODEX_MODEL:-gpt-6.2-sol}" "$CRC" "$T/codex.out" "$T/codex.stderr" review
```

---

## Integration Notes

**Called by**: `/task:orchestrate` after `/design-doc:create` completes

**Creates**:
- `docs/reviews/clo-XX-review-gemini.md` (always)
- `docs/reviews/clo-XX-review-codex.md` (always)
- `docs/reviews/clo-XX-review-claude-fallback.md` (only when both external models failed)
- `docs/reviews/clo-XX-review-synthesis.md` (always)
- `docs/reviews/clo-XX-review-{persona}.md` (if --persona used)

**Updates**: Nothing (read-only analysis)
