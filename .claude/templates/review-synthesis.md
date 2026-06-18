# Review Synthesis Template

**Purpose**: Consolidate findings from multiple AI reviewers (Gemini, Ollama, Claude fallback, optional personas) into a single actionable document. Adapts format based on how many reviewers succeeded.

---

## Review Synthesis: CLO-XX - [Title]

**Synthesized**: [Date]
**Design Document**: docs/design-docs/clo-XX-[description].md

---

## Reviewer Status

Always include this table first. Shows which reviewers ran, succeeded, or failed with reasons.

| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini 3.1 Pro | [OK / REVIEW_FAILED / SKIPPED] | [If failed: reason from REVIEW_FAILED line. If skipped: "Pre-flight check failed"] |
| Codex/Ollama (glm-5:cloud) | [OK / REVIEW_FAILED / SKIPPED] | [Same] |
| Claude (fallback) | [OK / SKIPPED] | [If OK: "Both external models failed, fallback activated". If skipped: "External reviewers succeeded"] |

---

## Format A: Multi Review (2+ reviewers succeeded)

Use this format when two or more reviewers produced valid reviews.

### Agreement (High Confidence)

Items where 2+ reviewers independently identified the same concern or strength. These are high-confidence findings.

| # | Finding | Reviewers | Severity |
|---|---------|-----------|----------|
| 1 | [finding] | Gemini, Ollama | [CRITICAL/HIGH/MEDIUM/LOW] |

### Disagreement (Needs Human Decision)

Items where reviewers hold divergent positions. Present both sides for the user to decide.

| # | Topic | Position A (Reviewer) | Position B (Reviewer) |
|---|-------|----------------------|----------------------|
| 1 | [topic] | [position] (Gemini) | [position] (Ollama) |

### Novel Insights (Single Reviewer)

Items found by only one reviewer. Lower confidence but may surface blind spots the other missed.

| # | Finding | Reviewer | Severity |
|---|---------|----------|----------|
| 1 | [finding] | Gemini | [severity] |

---

## Format B: Single Review (exactly 1 reviewer succeeded)

Use this format when only one reviewer produced a valid review (common when both external models fail and the Claude fallback activates).

### Source

[Which reviewer produced this review, and why others failed - reference the Reviewer Status table]

### Key Findings

| # | Finding | Severity |
|---|---------|----------|
| 1 | [finding] | [CRITICAL/HIGH/MEDIUM/LOW] |

**Note**: Single-reviewer findings have lower confidence than multi-reviewer agreement. The user should apply additional scrutiny.

---

## Shared Sections (both formats)

### Persona Summary (if applicable)

| Persona | Verdict | Key Finding |
|---------|---------|-------------|
| Security | [SAFE/CONCERNS_HIGH/CONCERNS_MEDIUM] | [1-line summary] |
| Concurrency | [SAFE/CONCERNS_HIGH/CONCERNS_MEDIUM] | [1-line summary] |
| Backend Integration | [CORRECT/CONCERNS_HIGH/CONCERNS_MEDIUM] | [1-line summary] |

### Consolidated Verdict

**Consensus Rules**:
- If ANY reviewer says NEEDS_REVISION -> Overall: NEEDS_REVISION
- If all say APPROVE -> Overall: APPROVE
- Otherwise -> Overall: APPROVE_WITH_SUGGESTIONS
- Single-reviewer verdicts carry a "(single source)" qualifier

**Overall Verdict**: [APPROVE | APPROVE_WITH_SUGGESTIONS | NEEDS_REVISION] [(single source) if only 1 reviewer]

### Priority Actions

Ordered by severity, with agreement items first (if multi-review):

1. **[CRITICAL]** [action] (agreed by: [reviewers])
2. **[HIGH]** [action] (source: [reviewer])
3. **[MEDIUM]** [action] (source: [reviewer])
