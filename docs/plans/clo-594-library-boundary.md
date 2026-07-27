# Plan: CLO-594 Lock the gcm library boundary, the sync/async seam and the config shape (ADR)

## Context
- Design: docs/designs/clo-594-library-boundary.md
- Discovery: docs/discovery/clo-594.md
- ADR: docs/adrs/002-library-boundary.md (Status: Accepted)
- Linear: https://linear.app/cloud-ai/issue/CLO-594/lock-the-gcm-library-boundary-the-syncasync-seam-and-the-config-shape
- Branch: feat/clo-594-lock

## Task type

This is an ADR-only task. No code moves under this issue (`*.rs` files unchanged). The deliverable is ADR-002 and its supporting documentation. The "implementation" is the ADR document itself, which is already written and accepted. The plan enumerates the remaining steps to finalize and merge.

## Sub-tasks

### ST1 Verify ADR-002 acceptance criteria
**Files:** docs/adrs/002-library-boundary.md
**Acceptance:** Each of the 4 issue ACs is met:
1. For each decision the ADR names the option chosen and the option rejected, with the reason — verify all 7 decisions have chosen + rejected + reason.
2. A reader can tell from the ADR alone where `Config`, `ProviderId` and `SecretScanMode` live after the extraction — verify the Library Surface section.
3. The sync/async decision is stated against lok's async `Backend` as settled in CLO-593 — verify Decision 2 references CLO-593.
4. No code moves under this issue — `git diff main..HEAD -- '*.rs'` is empty.
**Estimate:** S

### ST2 Verify no code changes
**Files:** (none — verification only)
**Acceptance:** `git diff main..HEAD -- '*.rs'` produces no output (only docs/ files changed)
**Estimate:** S

### ST3 Commit ADR and supporting docs
**Files:** docs/adrs/002-library-boundary.md, docs/adrs/README.md, docs/discovery/clo-594.md, docs/designs/clo-594-library-boundary.md, docs/reviews/clo-594-review-gemini.md, docs/reviews/clo-594-review-synthesis.md, docs/PROJECT.md, docs/ROADMAP.md
**Acceptance:** `git diff --cached --stat` shows only docs/ files; commit message references CLO-594
**Estimate:** S

### ST4 Create PR
**Files:** (PR creation via gh)
**Acceptance:** `gh pr create` succeeds; PR body references CLO-594 and lists the ADR decisions; PR is linked to Linear
**Estimate:** S

## Pre-merge gate
- N/A — no code changes. `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` runs on the existing codebase (unchanged). The PR is docs-only.

## Risks
- **Risk:** The ADR might need revision after CLO-595 implementation reveals issues. **Mitigation:** ADR-002 includes "Implementation Notes" from the Gemini review and "Condition for revisiting" clauses. If needed, supersede with ADR-003.
- **Risk:** The `blocked_by: CLO-593` relationship might cause confusion since CLO-593 is not in DEPENDENCIES.md. **Mitigation:** The ADR references CLO-593 as the predecessor; the sync/async decision is explicitly stated against CLO-593.