# Spec Review: clo-486

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-06-19
**Pipeline**: lok spec-review

---

## Specification Review: CLO-486 Single-Commit Tracer

### 1. Problem Statement Assessment

**Strengths:**
- Clearly defined and self-contained as the "tracer bullet" - thinnest end-to-end proof of the pipeline
- Explicitly states in-scope vs. deferred (no grouping, no plan cache, no provider trait)
- Matches Linear task description and includes FR references
- Deliberate departures from bash fallback documented (FR-10 for HTTP, FR-47 for read-only gather)
- Safety requirements clearly articulated (FR-32, FR-48, FR-57)

**Gaps:**
- Implications for testing strategy (no Rust code exists yet) could be clearer
- Downstream blocker unblocking criteria (CLO-487, 488, 489, 490) not explicitly specified

### 2. Acceptance Criteria Review

**Strong ACs:** AC-1 (signed commit with GPG/SSH and unicode handling), AC-2 (transactional abort with porcelain comparison), AC-3 (gitignore safety), AC-5 (no-change + non-repo), AC-6 (missing key), AC-9 (exit codes), AC-10 (no LLM CLI)

**Issues:**
- AC-4 (bounded I/O): "< ~3 s" is environment-dependent; needs minimum cap threshold specification
- AC-7 (edit path): Marked manual but unit test via $EDITOR script is feasible
- AC-1: Missing automated conventional commit format verification
- No AC for Groq HTTP 429/5xx errors - should exit cleanly
- No AC for git commit -S failures (signing key unavailable, pre-commit hook rejection)

### 3. Constraints Check

**Aligned with codebase:** Shell to git binary (ADR-001 #1), blocking HTTP with ureq (ADR-001 #2), GROQ_API_KEY env var (FR-18), --exclude-standard (FR-48), NUL-delimited parsing (FR-31), distinct exit codes (FR-39), no LLM CLI subprocess (FR-10), FR-49 egress disclosure

**Missing constraints:**
- No explicit timeout for Groq HTTP call (should specify default, e.g., 30s)
- No constraint on temp file cleanup across all paths (success, abort, error)
- Missing: $EDITOR fallback behavior should be constraint, not implementation note
- Missing: index restoration guarantee on Groq errors
- No constraint on prompt construction max diff size or file ordering

**Minor violation:** `--all` described as "no-op alias" contradicts PRD FR-6

### 4. Decomposition Quality

**Well-scoped sub-tasks:** 1-6 are cleanly partitioned and mostly independent

**Issues:**
- Sub-task 3 (diff gather): Cap logic algorithm underspecified (50 files / ~256 KB - what if first 50 are small but file 51 is 10MB?)
- Sub-task 5 (UI): Non-TTY handling contradicts ADR-001 #10 (spec says out-of-scope, ADR says error required)
- Missing sub-task: `scripts/acceptance.sh` is referenced but not explicitly scoped
- Sub-task 4 (Groq): Error taxonomy not specified - which GroqError variants exist?
- Dependency order correct but implicit: sub-task 3 calls sub-task 2 functions

### 5. Evaluation Coverage

**Covered:** Tests 1, 2, 3, 5, 6, 8, 9, 10

**Missing tests:**
- HTTP timeout behavior (Groq hangs)
- HTTP 4xx/5xx error handling
- Empty/whitespace Groq response
- Large untracked directory content verification
- Editor launch failure ($EDITOR non-existent)
- Commit failure (signing/pre-commit rejection per FR-58)
- Test #7 automated version (not just manual)
- Unborn branch (first commit against empty tree)
- File misclassified as text despite NUL bytes

### 6. Codebase Alignment

Greenfield Rust project. Correct ADR-001 alignment for git access, HTTP client, API key env var, default provider, and model selection.
