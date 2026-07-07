# Spec Review Synthesis: clo-533

**Synthesized**: 2026-07-07
**Pipeline**: lok spec-review

---

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Both external reviewers (Gemini, Ollama) succeeded and independently returned APPROVE_WITH_SUGGESTIONS. Claude fallback was correctly skipped. No reviewer returned NEEDS_REVISION, and no `REVIEW_FAILED` markers were present. The two reviews are complementary rather than contradictory: both confirm the spec correctly scopes Phase 2 as a thin orchestration layer over the Phase 1 core, and both converge on one blocking gap (clean-merge handling).

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Clean-merge path is missing.** `resolve::run_resolve_in_repo` currently aborts with `NoConflictInProgress` / `NoConflicts` when the source branch merges cleanly. The core must accept a flag (or path) that treats a conflict-free merge as success, returning `noop`/`resolved` with a zero exit, no LLM call. | Critical |
| 2 | **New `GcmError` variants are required and not yet in `src/error.rs`.** Orchestration errors (`RemoteHost`, `RemoteCliMissing`) must extend the central `GcmError` enum; do not fall back to bare `anyhow`. | High |
| 3 | **ST3 is under-decomposed.** "Fetch, checkout, merge, invoke engine" hides several distinct operations with independent error paths (scratch clone -> fetch branches -> resolution branch/merge -> invoke engine), and carries the hidden dependency on the clean-merge mechanism from #1. | Medium |
| 4 | **Dry-run purity needs tightening.** Both want AC8 to state explicitly that no temp dir is created, no clone happens, and no `gh`/`glab` is invoked. Ollama additionally wants `--dry-run --remote-push` behavior specified and tested (rejected vs silently ignored). | Medium |
| 5 | **Core entry-point refactor is non-trivial.** Reusing `run_resolve_in_repo` requires refactoring away from the current `run_resolve(args: &Cli)` that self-discovers the repo. Ollama supplied the concrete target signature `run_resolve_in_repo(repo: &Repo, args: &Cli) -> Result<ResolveReport, GcmError>`. | Medium |

---

## Disagreement (Needs Human Decision)

No direct contradictions between reviewers. One nuance on approach, not a genuine conflict:

| # | Topic | Gemini Position | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|-----------------|
| 1 | `GcmError` variant shape | Extend enum with orchestration variants (e.g. `RemoteHost`); shape left unspecified | Must use **structured fields** matching existing pattern: `RemoteHost { host, reason }`, `RemoteCliMissing { cli, install_hint }` - not `RemoteHost(String)` | Skipped (fallback not run) |

Ollama's structured-field position is the stronger one and aligns with existing variants like `ResolutionEscalated { path, reason }`. Recommend adopting it; no human decision truly required.

---

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **`RemoteReport` struct is undefined.** `ResolveReport` has no `remote` field; the JSON shape (host, number, base/source/resolution branch, pushed, commented) must be specified. | Ollama | High |
| 2 | **Scratch cleanup invariant has no AC.** No criterion requires the temp dir be removed on all exit paths (success, error, abort). Add AC13 + test; consider SIGINT/SIGTERM leaving partial state. | Ollama | High |
| 3 | **Git credential helper injection for HTTPS push.** Inside the scratch clone, `git push` over HTTPS fails unless configured to hook the host CLI (e.g. `git config credential.helper "!gh auth git-credential"`). | Gemini | High |
| 4 | **Self-hosted GHE / GitLab custom domains untested.** Host parsing must handle e.g. `git@gitlab.company.corp:group/repo.git` via domain heuristics, not just github.com/gitlab.com. | Gemini | Medium |
| 5 | **Stderr isolation.** `gh`/`glab`/`git` stderr must be captured separately and routed to stderr, or it corrupts `--json` stdout consumers. | Gemini | Medium |
| 6 | **No timeout on shell-out CLI ops.** Unlike HTTP `TIMEOUT_SECS`, a hung `gh pr checkout` could block indefinitely. | Ollama | Medium |
| 7 | **Resolution branch name collision.** If `gcm-resolve-github-123` already exists, behavior is undefined (checkout -B overwrite vs fail with guidance). Document it. | Ollama | Medium |
| 8 | **Concurrent-invocation scratch collision.** Two simultaneous `--pr 123` runs could collide on a deterministic path; spec is ambiguous on deterministic vs random. | Ollama | Medium |
| 9 | **Auth precondition handling.** If `gh`/`glab` are unauthenticated, orchestration must fail gracefully with actionable setup hints (EC4 recovery path is unspecified). | Gemini | Medium |
| 10 | **Partial-resolution scenario untested.** AC10 mentions escalation but no evaluation row covers "some files resolved, others escalated." | Ollama | Medium |
| 11 | **Custom SSH key / ssh-agent preservation** for SSH-origin scratch clones. | Gemini | Low |
| 12 | **Test file naming mismatch** (`tests/resolve_remote.rs` vs existing `tests/resolve_integration.rs` convention). | Ollama | Low |
| 13 | **Min `gh`/`glab` version** for `pr checkout --branch` undocumented; no fallback. | Ollama | Low |
| 14 | **Provider `diff_budget` for remote conflicts** unaddressed - should it differ from local? | Ollama | Low |

---

## Priority Actions

Ordered by severity, agreement items first.

**P0 - Blocking (fix before implementation)**
1. **Refactor core for clean merges (Agreement #1).** Add a parameter to `run_resolve_in_repo` so a conflict-free merge returns `noop`/`resolved` with exit 0 instead of `NoConflictInProgress`/`NoConflicts`. Add an evaluation row asserting this path (no LLM call, no conflicts in resolution branch). This is the one finding both reviewers escalate.
2. **Define `RemoteReport` struct + `ResolveReport.remote` field (Novel #1).** Specify the concrete JSON shape in the spec before wiring ST5.
3. **Add `GcmError` variants with structured fields (Agreement #2 + Disagreement #1).** `RemoteHost { host, reason }`, `RemoteCliMissing { cli, install_hint }` in `src/error.rs`, following the `ResolutionEscalated` pattern.

**P1 - Should fix**
4. **Add scratch-cleanup AC (Novel #2).** AC13: temp dir removed on all exit paths; test cleanup-on-error. Note SIGINT behavior.
5. **Tighten dry-run (Agreement #4).** State in AC8: no temp dir, no clone, no CLI invocation; specify and test `--dry-run --remote-push`.
6. **Credential-helper injection in ST3 (Novel #3).** Configure the scratch clone to reuse the host CLI credential helper for HTTPS push.
7. **Decompose ST3 (Agreement #3)** into scratch-clone / fetch / merge / invoke sub-tasks, each with its error path; make the core-signature refactor (Agreement #5) an explicit sub-task with the target signature.

**P2 - Completeness**
8. Self-hosted/custom-domain host parsing + test (Novel #4).
9. Stderr isolation to protect `--json` stdout (Novel #5).
10. Branch-name collision + concurrent-scratch-collision handling and tests (Novel #7, #8); pin deterministic vs random path choice.
11. Partial-resolution and auth-failure (EC4 recovery) evaluation scenarios (Novel #9, #10).

**P3 - Refinement**
12. Timeout wrapper for shell-out ops (Novel #6); document min CLI versions (#13); align test file naming (#12); decide remote `diff_budget` (#14); SSH-agent preservation (#11).
