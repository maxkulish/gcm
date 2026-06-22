# CLO-490 Implementation Plan: Optional Secret Scanning and gcmignore Path Filtering

**Linear Issue**: https://linear.app/cloud-ai/issue/CLO-490/add-optional-secret-scanning-and-gcmignore-path-filtering
**Design Document**: docs/design-docs/2026-06-22-clo-490-secret-scan-gcmignore.md
**Architecture Reference**: docs/adrs/001-foundational-architecture-decisions.md
**Created**: 2026-06-22
**Overall Progress**: 100% (47/47 tasks completed)

---

## Architecture Context

Provider egress is assembled in `src/diff.rs` and sent from `src/main.rs` through the `Provider` trait. CLO-490 adds a privacy layer before those calls while preserving provider backends, cache semantics, and whole-file staging behavior for nonignored files.

---

## Tasks

### Phase 1: Privacy Module

- [x] Task 1: Add `src/privacy.rs`
  - [x] Implement `.gcmignore`/`gcmignore` loading with comments and blank lines ignored.
  - [x] Implement dependency-free glob matching for exact paths, directory prefixes, `*`, and `?`.
  - [x] Add built-in filtering for `.gcmignore` and `gcmignore`.

- [x] Task 2: Add secret scan primitives
  - [x] Support `off`, `redact`, and `abort` modes.
  - [x] Detect common token shapes and credential assignments.
  - [x] Redact detected values without sending originals.

### Phase 2: Runtime Wiring

- [x] Task 3: Wire CLI and env configuration
  - [x] Add `--secret-scan=off|redact|abort`.
  - [x] Resolve `GCM_SECRET_SCAN` with flag precedence.
  - [x] Add config/secret-detected error codes.

- [x] Task 4: Filter changed files before analysis
  - [x] Apply filtering after merge-conflict detection and before cache/validation.
  - [x] Return noop when every change is ignored.
  - [x] Ensure ignored paths do not affect cache freshness.

- [x] Task 5: Scope provider-bound diffs
  - [x] Change grouping gather to use the filtered changed-file path set.
  - [x] Add a filtered single-commit gather for `--all` and fallback.
  - [x] Keep cache-hit message generation scoped to group files.

- [x] Task 6: Apply secret scan before every provider call
  - [x] Grouping plan call.
  - [x] Group message call on cache hit.
  - [x] Single-commit/fallback message call.

- [x] Task 7: Keep commit behavior aligned with filtered changes
  - [x] Stage only filtered files in single-commit and fallback paths.
  - [x] Keep grouped staging unchanged for filtered plan groups.

### Phase 3: Docs and Tests

- [x] Task 8: Unit tests
  - [x] Pattern matching and changed-file filtering.
  - [x] Secret redaction and abort detection.
  - [x] Error-code mapping.

- [x] Task 9: Acceptance tests
  - [x] Prove ignored path/content never reaches mock provider capture.
  - [x] Prove redact mode removes credential values before egress.
  - [x] Prove abort mode sends no provider request.

- [x] Task 10: Documentation
  - [x] Update README privacy/config sections.
  - [x] Update CLI help disclosure.

### Phase 4: Validation and Finalization

- [x] Task 11: Run format and unit tests
  - [x] `cargo fmt --check`
  - [x] `cargo test`

- [x] Task 12: Run lint and acceptance
  - [x] `cargo clippy --all-targets -- -D warnings`
  - [x] `scripts/acceptance.sh`

- [x] Task 13: Update workflow/project state
  - [x] Mark implementation results in `docs/status/clo-490-workflow.yaml`.
  - [x] Leave CLO-490 in active/PR-ready project state until a PR is opened and merged.

---

## Module Structure

- `src/privacy.rs` - new privacy layer.
- `src/cli.rs` - secret scan CLI option and help text.
- `src/diff.rs` - filtered gather helpers.
- `src/main.rs` - orchestration and staging changes.
- `src/error.rs`, `src/output.rs` - error display/JSON codes.
- `README.md`, `scripts/acceptance.sh` - docs and e2e checks.

---

## Risk Notes

- Glob support is intentionally small; negation and nested ignore files are out of scope.
- Secret scanning is best-effort and opt-in; `abort` is the strictest available mode.
- Single-commit/fallback must not call `git add -A`, or ignored paths could be committed despite being excluded from analysis.
