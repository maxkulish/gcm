# Plan: CLO-531 — `gcm resolve` LLM-assisted merge conflict resolver (Phase 1: local markers)

**Linear:** [CLO-531](https://linear.app/cloud-ai/issue/CLO-531/add-gcm-resolve-llm-assisted-merge-conflict-resolver-phase-1-local)
**Design:** `docs/designs/clo-531-gcm-resolve.md`
**Discovery:** `docs/discovery/clo-531.md`
**PRD:** `docs/prds/clo-531-gcm-resolve.md`
**Branch:** `feat/clo-531-resolve`
**Date:** 2026-07-06

---

## Goal

Implement the `gcm resolve` subcommand end-to-end, following the layered local pipeline from the design doc. Each sub-task is independently testable and builds on the previous one.

---

## Sub-tasks

### ST1 Extend git plumbing for conflict enumeration and zdiff3 re-checkout
**Files:** `src/git.rs`, `src/error.rs`
**What:** Add `Repo::unmerged_files()`, `Repo::checkout_conflict_zdiff3()`, `Repo::read_file()`, `Repo::write_file()`, plus binary-file detection (`git diff --numstat`). Add `GcmError::NoConflictInProgress`, `GcmError::NoConflicts`, `GcmError::ResolutionEscalated`.
**Acceptance:** `cargo test git::` (existing + new unit tests for NUL-delimited paths and binary detection) passes.
**Estimate:** S

### ST2 Implement zdiff3 marker parser
**Files:** `src/resolve/markers.rs`, `src/resolve/mod.rs` (module declaration)
**What:** Parse `zdiff3` conflict markers into `Hunk { start_line, end_line, base, ours, theirs }` and `ConflictFile`. Support multiple hunks, missing base, and context-line preservation. Add unit tests.
**Acceptance:** `cargo test resolve::markers` passes (tests: single hunk, multiple hunks, no base, no conflicts).
**Estimate:** S

### ST3 Implement hunk classifier and validation gate
**Files:** `src/resolve/classify.rs`, `src/resolve/validate.rs`
**What:** Implement trivial/complex classification (`classify`) with `AutoReason`. Implement `validate(resolved_text, validate_cmd, repo, path)` with syntax-safe default (no markers) and optional `validate_cmd` via temp file + repo-root cwd.
**Acceptance:** `cargo test resolve::classify && cargo test resolve::validate` passes.
**Estimate:** S

### ST4 Implement resolution report envelope
**Files:** `src/resolve/report.rs`, `src/output.rs`
**What:** Add `ResolveReport`, `ResolveStatus`, `FileReport`, `FileAction` types and wire them into the existing envelope machinery so `--json` can emit a single JSON object on stdout.
**Acceptance:** `cargo test resolve::report` passes and a manual `gcm resolve --json --dry-run` in a conflicted repo prints only JSON to stdout.
**Estimate:** S

### ST5 Add `[conflict]` config block and CLI subcommand
**Files:** `src/config.rs`, `src/cli.rs`, `src/main.rs`
**What:** Add `ConflictConfig` (temperature, validate_cmd, sensitive_paths, auto_policy, mergiraf) and CLI `Commands::Resolve` with `--conflict-temperature`, `--conflict-validate-cmd`, `--conflict-auto-policy`, `--conflict-sensitive-paths`, `--no-mergiraf`. Add `main.rs` dispatch branch with an early return so other subcommands are untouched.
**Acceptance:** `cargo test cli:: && cargo test config::` passes; `gcm resolve --help` shows the new flags.
**Estimate:** S

### ST6 Extend Provider trait with `resolve_hunks`
**Files:** `src/provider/mod.rs`, all provider backend modules
**What:** Add required `Provider::resolve_hunks(&self, ctx: &ResolveContext) -> Result<Vec<HunkResolution>, ProviderError>` and `ResolveContext`. Implement the method for all five backends using each provider's native structured-output mechanism (OpenAI `response_format`, Gemini `responseSchema`, Anthropic forced tool, Ollama `format`). Reuse `strip_think` and `parse_defensive`.
**Acceptance:** `cargo test provider::` passes; all backends compile; at least one backend has a unit test for the resolution JSON schema.
**Estimate:** M

### ST7 Implement optional `mergiraf` pre-resolution stage
**Files:** `src/resolve/mergiraf.rs`
**What:** Detect `mergiraf` on PATH, run `mergiraf --ast <file>` per file, re-parse residual markers. If not on PATH or `--no-mergiraf`, skip silently.
**Acceptance:** `cargo test resolve::mergiraf` passes including tests for `is_available()` false when binary missing and graceful skip behavior.
**Estimate:** S

### ST8 Implement 3-way resolution prompt and anti-hallucination rules
**Files:** `src/resolve/prompt.rs`
**What:** Build the per-hunk 3-way prompt (base, ours, theirs, style context, anti-hallucination rules, JSON schema). Keep prompt tokens within the provider diff budget.
**Acceptance:** `cargo test resolve::prompt` passes and a unit test asserts the prompt contains the required marker instructions and schema hints.
**Estimate:** S

### ST9 Implement resolve orchestrator and per-file preview loop
**Files:** `src/resolve/mod.rs`, `src/ui.rs` (minor edit for per-file confirm)
**What:** Wire the full pipeline: detect conflict state → re-checkout zdiff3 → binary skip → parse markers → optional mergiraf → classify → privacy filter → provider resolution → validation → per-file `[Y/n/e]` preview loop → report. Implement `--dry-run`, `--yes`, `--json`. Never `git add` or `git merge --continue`.
**Acceptance:** `cargo test resolve::` passes (including orchestrator integration tests for trivial conflict, complex conflict, dry-run, JSON envelope, skip, escalation).
**Estimate:** L

### ST10 Add end-to-end integration tests and fix roll-out polish
**Files:** `tests/resolve_integration.rs` (new), `src/error.rs`, `src/output.rs` (final wiring)
**What:** Add integration tests: trivial conflict, one-side-unchanged, complex conflict, mergiraf absent, dry-run, JSON envelope, yes non-interactive, skip, edit, validation retry then escalate, no merge in progress, no conflicts, binary file skipped, secret scan aborts, gcmignore excludes. Fix any final wiring issues.
**Acceptance:** `cargo test --test resolve_integration` passes.
**Estimate:** M

### ST11 Pre-merge gate
**Files:** all touched files
**What:** Run full project quality checks and ensure no regressions in existing commands.
**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` passes.
**Estimate:** S

---

## Pre-merge gate

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

---

## Risks

1. **Provider trait breakage.** Adding a required `resolve_hunks` method will break each backend until implemented. Mitigation: implement for all five backends in the same ST6 commit and run `cargo check --all-targets` before committing.
2. **`mergiraf` CLI semantics.** Discovery debt notes the exact exit-code / residual-marker contract is unverified. Mitigation: ST7 includes graceful re-parsing; if `mergiraf` fails, we fall back to the LLM path.
3. **LLM context window.** Large files with many complex hunks could exceed provider limits. Mitigation: design doc § "Context window management" batching is implemented in ST9.
4. **Validation command environment.** `validate_cmd` may depend on the repo's toolchain (e.g. `cargo check`). Mitigation: run `validate_cmd` from `repo.root()` with a temp file path argument (ST3).
5. **Regression in existing commands.** Adding a new subcommand and error variants could affect existing flows. Mitigation: ST11 full test suite + clippy gate.

---

## Order summary

1. ST1 — git plumbing + errors
2. ST2 — marker parser
3. ST3 — classifier + validation
4. ST4 — report envelope
5. ST5 — config + CLI + main dispatch
6. ST6 — Provider trait extension (all backends)
7. ST7 — mergiraf stage
8. ST8 — resolution prompt
9. ST9 — orchestrator + preview loop
10. ST10 — end-to-end integration tests
11. ST11 — pre-merge gate
