# CLO-537: Add Vertex AI provider (keyless ADC)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-537
**Plan File**: docs/plans/clo-537-vertex-provider.md
**Design Document**: docs/designs/clo-537-vertex-provider.md
**Started**: 2026-07-09
**Last Updated**: 2026-07-09

---

## Current Status: Code Complete (validation gate + live HITL remain)

**Overall Progress**: 94% (91/97 tasks)
**Current Phase**: Phase 7 done -> Codex/Gemini validation gate, then PR
**Commit**: 13a0e29

All 8 code phases implemented and committed. Remaining: live HITL verify (Task 24,
needs the maintainer's GCP project) and PR creation (Task 25, the orchestrator's PR
phase). Gates: 422 tests pass, fmt + clippy clean.

---

## Session Log

### Session 1 - 2026-07-09

**Branch**: feat/clo-537-vertex

Implementing from the finalized design doc + approved plan (both passed 2 rounds of
code-validated owner review). Executing phase-by-phase with `cargo build/test/clippy/fmt`
gates and a commit per phase; surfacing at the Codex+Gemini validation gate before PR.

#### Tasks Completed This Session

(populated as tasks complete)

---

## Technical Decisions

- **401/403 error mapping mechanism**: `http.rs::classify_status` derives `Auth{env_var}`
  vs `Http(status)` from whether `req.auth` is `Some` (http.rs:127/187), not from
  `auth_env_var` directly. So Vertex sends the Bearer token via `extra_headers` with
  `auth: None`, making 401/403 surface as `Http(status)`, which `vertex.rs` re-maps to
  actionable text. No shared-code change (cleaner than the design's "ErrorKind arm").
- **Reuse of `gemini::extract_text`**: kept as-is (pub(super)); its errors carry provider
  name "Google" - acceptable since Vertex serves the same Gemini models (the extractor
  messages say "Gemini blocked...", which is accurate).
- **No regex dep** (ADR-001 ethos): location/project validation is manual char checks.

---

## Next Steps

Phase 1 compile-gate: enum variant + all exhaustive match arms land together.
