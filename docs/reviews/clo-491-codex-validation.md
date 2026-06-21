Reading additional input from stdin...
OpenAI Codex v0.141.0
--------
workdir: /Users/mk/Code/gcm--feat-clo-491-plan-cache
model: gpt-5.4
provider: openai
approval: never
sandbox: read-only
reasoning effort: xhigh
reasoning summaries: none
session id: 019ee683-b818-7ac0-ad53-a410a0ffbeb3
--------
user
You are a senior Rust code reviewer. Review all changes on this branch against the design document and implementation plan for CLO-491 (per-repo plan cache with commit-safe advancement). This is a read-only review: do not modify any files.

FILES TO READ:
1. Design doc: docs/design-docs/2026-06-20-clo-491-plan-cache.md
2. Implementation plan: docs/plans/clo-491-plan-cache.md
3. Run: git diff main...HEAD  (all changes; focus on src/)
4. Read in full: src/cache.rs, src/main.rs, src/diff.rs, src/git.rs, src/error.rs, src/groq.rs, src/cli.rs, src/plan.rs

CHECK FOR (CLO-491 specifics):
1. CORRECTNESS vs the design (FR-2/8/25-30/45/58):
   - The freshness fingerprint must NEVER pin HEAD, must be recomputed on advance, and must survive a commit (committing group 0 must not invalidate the remaining cache) - FR-26/FR-27.
   - A cache HIT must make NO grouping (response_format) call; an advanced group's null commit_message is regenerated via a message-only call scoped to that group's diff (ADR-001 #6), taken BEFORE staging.
   - A commit failure (GcmError::CommitFailed) must leave the group STAGED and the cache UN-advanced (FR-58); the index is restored only on pre-commit-step failures (FR-47).
   - cache::advance must be gated on CommitOutcome::Committed - never on abort or --dry-run.
   - --reset clears the cache up front; --all and the single-commit fallback clear it. The cache is best-effort: corrupt/missing -> re-analyze, write failure -> warn; it must NEVER abort a commit.
2. COMPLETENESS: are all 7 acceptance criteria (AC-1..AC-7) covered by tests (unit + scripts/acceptance.sh AC-C1..AC-C7, AC-C11, AC-C21)?
3. REGRESSIONS: could adding the cache break the existing grouping, single-commit, merge-guard, or fallback paths?
4. CODE QUALITY: error handling, dead code, idiomatic Rust, naming consistent with the existing modules.
5. SECURITY: 0600 set before content lands; no secrets written to the cache; safe path handling (no traversal); symlink/binary handling in content_hash.
6. CONCURRENCY / EDGE CASES: atomic write (temp + rename), corrupt/truncated cache file, large files (streaming hash - no OOM), unborn branch, deletions/renames folded into the fingerprint, GCM_CACHE_DIR override.

OUTPUT FORMAT:
## Verdict: [PASS | PASS_WITH_NOTES | FAIL]
## Findings
[each finding with severity CRITICAL / HIGH / MEDIUM / LOW, file:line, and why]
## Missing Items
[any acceptance criteria or edge cases not implemented or not tested]
## Recommendations
[specific, actionable]
codex
Reviewing the branch against the CLO-491 design and plan first, then I’ll trace the `main...HEAD` diff through the affected Rust modules and test coverage to look for correctness gaps and regressions.
exec
/bin/zsh -lc "sed -n '1,260p' docs/design-docs/2026-06-20-clo-491-plan-cache.md" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
exec
/bin/zsh -lc "sed -n '1,260p' docs/plans/clo-491-plan-cache.md" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
# CLO-491 Implementation Plan: Per-repo plan cache with commit-safe advancement

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-491/add-per-repo-plan-cache-with-commit-safe-advancement
**Design Document**: docs/design-docs/2026-06-20-clo-491-plan-cache.md
**Workflow State**: docs/status/clo-491-workflow.yaml
**Created**: 2026-06-20
**Overall Progress**: 88% (Tasks 1-15 complete; Task 16 validation + Phase 5 PR remaining)

---

## Architecture Context

A new in-process `src/cache.rs` module sits between the grouping call and the commit flow in
`main.rs`, owning one JSON file per repo (`sha256(repo-root)` under the OS cache dir, `0600`).
It mirrors the bash plan cache (`git-commit-ai.sh:65-67/283-297/470-478`) but fixes its two
defects - name-only staleness (-> content fingerprint, FR-27) and null-message advancement
(-> regenerate-per-group, ADR-001 #6). The cache is best-effort: it never blocks a commit.
Implementation order respects dependencies: deps + `Plan: Serialize` -> cache module ->
provider/diff helpers -> error/cli/git -> `main.rs` wiring -> tests.

TDD discipline (CLO-487 pattern): for pure-logic units write a failing test first
(stub -> RED -> impl -> GREEN) and confirm each test is load-bearing with a mutation check;
integration via `scripts/acceptance.sh` against the mock-Groq harness.

---

## Tasks

### Phase 1: Dependencies & cache module

- [ ] Task 1: Add crates to `Cargo.toml`
  - [ ] Subtask 1.1: add `directories` (OS cache dir, ADR-001 #12) and `sha2` (key + content hash)
  - [ ] Subtask 1.2: `cargo build` to refresh `Cargo.lock`; confirm both resolve
- [ ] Task 2: `src/plan.rs` - make the plan cacheable
  - [ ] Subtask 2.1: add `Serialize` to the `#[derive(...)]` on `Plan` and `Group` (currently `Deserialize`-only)
  - [ ] Subtask 2.2: unit test: a `Plan` round-trips through `serde_json` (serialize -> deserialize) incl. a `null` later-group `commit_message`
- [ ] Task 3: `src/cache.rs` - path & key
  - [ ] Subtask 3.1: `cache_path(repo_root) -> Option<PathBuf>` via `ProjectDirs::from("", "", "gcm").cache_dir()` + `plan-<sha256(repo_root) hex>.json`
  - [ ] Subtask 3.2: create the cache dir if missing (`create_dir_all`); never fail the caller on a dir error (return `None`)
  - [ ] Subtask 3.3: unit test: full-hex key is stable for a path and differs across paths
- [ ] Task 4: `src/cache.rs` - fingerprint (FR-27)
  - [ ] Subtask 4.1: `FINGERPRINT_VERSION` + `CACHE_FORMAT_VERSION` consts; provider token `"groq:"` + `resolved_model()` (Task 6)
  - [ ] Subtask 4.2: `content_hash(repo, &ChangedFile)` - **streaming** `BufReader` + `io::copy` into `Sha256` (constant memory); `"\0DELETED"` marker for a pending deletion
  - [ ] Subtask 4.3: `fingerprint(repo, pending, model)` = sha256 over (version || provider_model || per sorted path: path||content_hash); pending read from LIVE `changed_files()`
  - [ ] Subtask 4.4: unit tests: same inputs -> same digest; content change, name-set change, model change, version bump each flip it; deletion marker distinguishes present vs deleted
- [ ] Task 5: `src/cache.rs` - read/write/advance/clear
  - [ ] Subtask 5.1: `CacheFile { version, fingerprint, plan }` (`Serialize`/`Deserialize`)
  - [ ] Subtask 5.2: `load(repo, model) -> Option<Plan>` - deserialize, reject wrong `version`, recompute fingerprint over live pending, return `Some` iff match (corrupt/garbage -> `None`)
  - [ ] Subtask 5.3: `save(repo, plan, model)` - atomic write: temp file in the cache dir, **chmod 0600 before writing**, then rename; best-effort (warn, never abort)
  - [ ] Subtask 5.4: `advance(repo, plan, model)` - drop `groups[0]`; delete file if no groups remain, else re-stamp fingerprint over the new pending set + write
  - [ ] Subtask 5.5: `clear(repo)` - remove the file, ignore "not found"
  - [ ] Subtask 5.6: unit tests: `load` rejects wrong format version + corrupt JSON; `advance` drops group 0 and deletes on empty

### Phase 2: Provider & diff helpers

- [ ] Task 6: `src/groq.rs` - `resolved_model()`
  - [ ] Subtask 6.1: extract the model resolution from `resolve_config` into `resolved_model() -> String` (reads `GCM_GROQ_MODEL` or default; **no API key required**)
  - [ ] Subtask 6.2: `resolve_config` reuses it; unit test: default + env override, no key set
- [ ] Task 7: `src/diff.rs` - filter `append_untracked`
  - [ ] Subtask 7.1: refactor `append_untracked` to take an allow-list (e.g. `Option<&HashSet<String>>` or `&[&str]`); `None`/all = current behavior
  - [ ] Subtask 7.2: `gather`/`gather_for_grouping` pass "all" (unchanged output - keep existing tests green)
  - [ ] Subtask 7.3: unit test: filtered call appends only allow-listed untracked paths
- [ ] Task 8: `src/diff.rs` - `gather_for_files`
  - [ ] Subtask 8.1: `gather_for_files(repo, &[&ChangedFile]) -> Result<GatheredDiff, GcmError>` - path-scoped tracked diff (literal NUL pathspecs, `core.quotePath=false`) + stat
  - [ ] Subtask 8.2: append untracked content **filtered to the group's paths** (Task 7); unborn-branch case (no `HEAD` -> all content via the untracked path)
  - [ ] Subtask 8.3: unit/integration test: scoped diff excludes other groups' files (tracked and untracked)

### Phase 3: Failure semantics, CLI, and `main.rs` wiring

- [ ] Task 9: `src/error.rs` - commit-failure variant
  - [ ] Subtask 9.1: add `GcmError::CommitFailed(String)` (Display surfaces the git error - FR-58) and `leaves_staged(&self) -> bool` (`true` only for `CommitFailed`)
  - [ ] Subtask 9.2: unit test: `leaves_staged()` true for `CommitFailed`, false for `Git`/others
- [ ] Task 10: `src/git.rs` - `commit_signed` returns `CommitFailed`
  - [ ] Subtask 10.1: on a non-zero `git commit`, return `GcmError::CommitFailed(...)` (not `GcmError::Git`); drop the now-false "index restored" wording. `clear_staged`/`stage_group` keep `GcmError::Git`
- [ ] Task 11: `src/cli.rs` - `--reset`
  - [ ] Subtask 11.1: add `#[arg(long)] pub reset: bool` (FR-8); short help "Discard any cached plan and re-analyze"
- [ ] Task 12: `src/main.rs` - orchestration
  - [ ] Subtask 12.1: `CommitOutcome { Committed, Aborted }`; `commit_group_flow` returns `Result<CommitOutcome, GcmError>` (Abort -> `Ok(Aborted)`, commit success -> `Ok(Committed)`)
  - [ ] Subtask 12.2: `--reset` / `--all` / grouping fallback call `cache::clear` (reset up front; all/fallback on the single-commit path)
  - [ ] Subtask 12.3: `cache::load` before `build_plan`; on hit skip grouping, on miss `generate_plan` -> `validate_basic` -> `cache::save` (full plan)
  - [ ] Subtask 12.4: message-on-hit - if `groups[0].commit_message` is null (advanced group), `groq::generate_commit_message(diff::gather_for_files(group0))` **before** staging; full-plan hit uses the cached message (zero LLM calls)
  - [ ] Subtask 12.5: narrow the restore-on-error wrapper in `commit_first_group` - restore only when `!e.leaves_staged()` (leave group staged on `CommitFailed`)
  - [ ] Subtask 12.6: `cache::advance` only on `CommitOutcome::Committed`
  - [ ] Subtask 12.7: `--dry-run` saves the plan on a miss but does not advance (FR-7)

### Phase 4: Testing & validation

- [ ] Task 13: Unit tests green
  - [ ] Subtask 13.1: `cargo test` - all new cache/diff/error/groq units pass; mutation-check the key fingerprint + advance + leaves_staged tests
- [ ] Task 14: Acceptance suite (`scripts/acceptance.sh`, mock-Groq harness)
  - [ ] Subtask 14.1: AC-1 cache hit -> group 2 commits, **zero grouping requests** on re-run (message-only allowed), valid message
  - [ ] Subtask 14.2: AC-2 edit a pending file -> re-analyze (fresh grouping call); AC (eval 4) rename -> re-analyze
  - [ ] Subtask 14.3: AC-3 rejecting pre-commit hook -> exit!=0, group staged, cache byte-identical, next run retries; eval 6 hook reformats+restages -> commit succeeds + advances
  - [ ] Subtask 14.4: AC-4 unborn-branch first commit with cache; AC-5 cache file under OS cache dir + mode 0600
  - [ ] Subtask 14.5: AC-6 `--reset` forces re-analysis; `--all`/fallback clear the cache; AC-7 abort -> cache un-advanced
  - [ ] Subtask 14.6: eval 11 single-group -> cache deleted; eval 17 deletion-only group 0; eval 18 untracked-only group 0; eval 21 untracked filter excludes other groups
- [ ] Task 15: Quality gate
  - [ ] Subtask 15.1: `cargo fmt --check` clean
  - [ ] Subtask 15.2: `cargo clippy --all-targets -- -D warnings` clean
  - [ ] Subtask 15.3: `cargo test` + `./scripts/acceptance.sh` all green; `cargo build --release` ok
- [ ] Task 16: Dual-model validation gate (read-only, HEAD unchanged - CLO-487 pattern)
  - [ ] Subtask 16.1: Gemini `--approval-mode plan` review of the diff vs the design ACs
  - [ ] Subtask 16.2: Codex `-s read-only` review; apply any HIGH/MEDIUM findings with regression tests; re-validate to convergence

### Phase 5: Finalization

- [ ] Task 17: Pull request
  - [ ] Subtask 17.1: pre-flight `cargo fmt --check` / `clippy -D warnings` / `cargo test` / clean tree
  - [ ] Subtask 17.2: push `feat/clo-491-plan-cache`; `gh pr create` against `main` with a body covering cache design, FR-58 safety, and the validation gate
  - [ ] Subtask 17.3: confirm CI green (macOS + Linux); link to Linear CLO-491

---

## Module Structure

- `src/cache.rs` - **new**: path/key, fingerprint (streaming), load/save/advance/clear
- `src/plan.rs` - `Serialize` on `Plan`/`Group`
- `src/groq.rs` - `resolved_model()`; reuse `generate_commit_message` for the per-group message
- `src/diff.rs` - `gather_for_files`; `append_untracked` allow-list filter
- `src/git.rs` - `commit_signed` -> `CommitFailed`
- `src/error.rs` - `GcmError::CommitFailed` + `leaves_staged()`
- `src/cli.rs` - `--reset`
- `src/main.rs` - cache wiring, `CommitOutcome`, narrowed restore-on-error
- `Cargo.toml` - `directories`, `sha2`
- `scripts/acceptance.sh` - AC-1..AC-7 + eval cases

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: edit this file and change checkboxes; recalculate the percentage from completed top-level tasks.

---

## Notes

- The cache is best-effort: corrupt/missing -> re-analyze; write failure -> warn and continue; never abort a commit (that path is reserved for FR-58 commit failures).
- `advance` is gated on `CommitOutcome::Committed` - never on abort/`--dry-run`.
- Fingerprint streams file content (constant memory) and never pins `HEAD`.
- Follow existing CLO-487 git-plumbing primitives (`changed_files`, literal NUL staging) - the cache reuses them, it does not re-parse status.
- Acceptance criteria (AC-1..AC-7) and the 21-row eval table live in the design doc; this plan maps them to Tasks 14.x.

 succeeded in 0ms:
# CLO-491: Add per-repo plan cache with commit-safe advancement

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-491/add-per-repo-plan-cache-with-commit-safe-advancement
**Status**: Finalized
**Author**: Max Kulish
**Created**: 2026-06-20
**Finalized**: 2026-06-20
**Approved By**: Max Kulish (owner)
**Slice**: S3 (extends CLO-487 grouping; message contract from CLO-485 ADR-001)
**Covers FR**: 2 (now full), 8, 25, 26, 27, 28, 29, 30, 45, 58

---

## Summary

Persist the grouping plan per repository so that re-runs commit the next group without
re-calling the grouping LLM, advancing one group per successful commit. Freshness is
decided by a content fingerprint over the not-yet-committed files (not file names alone),
re-stamped each time the cache advances; a failing `git commit` (e.g. a rejecting
pre-commit hook) leaves the plan un-advanced and the group still staged. This mirrors the
bash tool's plan cache while fixing its two latent defects: name-only staleness and the
null-message advancement bug that collapsed groups 2+ into single-commit-all.

---

## Background

CLO-487 shipped semantic grouping: a structured-output `Plan` is requested from Groq,
group 1 is staged and committed, and a re-run advances to the next group. But CLO-487 has
**no cache** - every re-run makes a fresh grouping call and re-derives the plan from
scratch. That is correct but wasteful: the plan rarely changes between commits, yet each
run pays a full grouping round-trip.

The bash reference (`docs/tmp/git-commit-ai.sh`) already had a per-repo cache, and CLO-491
mirrors it - but it carries two defects this task exists to fix:

- **Name-only staleness** (`git-commit-ai.sh:283-297`): the cache is reused when the sorted
  set of file *names* matches, ignoring content. Editing a pending file after a `--dry-run`
  (names unchanged) silently reuses a stale plan.
- **Null-message advancement** (`git-commit-ai.sh:470-478`): after advancing, the new first
  group's `commit_message` is `null` (only the original `groups[0]` carried a message), which
  in the bash tool tripped the single-commit-all fallback and collapsed grouping for groups
  2+.

The message contract is fixed by [ADR-001](../adrs/001-foundational-architecture-decisions.md)
Decision #6 (**regenerate-per-group**): only `groups[0]` carries a message from the initial
plan; each later group's message is generated on the run that commits it, scoped to that
group's diff. CLO-491 is where caching and that contract meet.

### Prior Research (Discovery)

Discovery was run **focused** (the project PRD is ADR-mature; the full multi-agent
`prd-discovery` was deliberately skipped - see `docs/status/clo-491-workflow.yaml`). The
solution space is effectively singular: mirror the bash cache structure and fix its two
bugs, using the ADR-001-locked decisions. Discovery surfaced **7 locked constraints** and
**4 residual design-level questions**, which this document resolves:

| # | Residual question | Resolution (this doc) |
|---|---|---|
| Q1 | On-disk cache schema | A `version` + `fingerprint` + `plan` wrapper struct; FR-30 bash compat dropped (ADR-001 #12). See [Cache file format](#cache-file-format). |
| Q2 | Content-hash inputs | sha256 over (sorted pending paths + per-file working-tree content hash + provider/model + prompt/schema version); HEAD never pinned; unborn-safe. See [Freshness fingerprint](#freshness-fingerprint-fr-27). |
| Q3 | Message on cache hit | Reuse `groq::generate_commit_message` scoped to the now-first group's diff, only when its cached message is `null` (advanced group). A grouping call is never made on a hit. See [Message on a cache hit](#message-on-a-cache-hit-fr-45-adr-001-6). |
| Q4 | Failure semantics | Narrow the restore-on-error wrapper: restore the index only for pre-commit-step failures (FR-47); on a `commit_signed` failure leave the group staged and the cache un-advanced (FR-58). See [Commit-safe advancement](#commit-safe-advancement-fr-26-fr-58). |

---

## Architecture

### Component Overview

The cache is a new in-process module (`src/cache.rs`) that sits between the grouping call
and the commit flow in `main.rs`. It owns one plain JSON file per repo. No async, no
network, no new long-lived state - the file *is* the state (Rust owns it, ADR-001).

```
                 ┌──────────────────────────── main.rs::execute ────────────────────────────┐
                 │                                                                             │
  git status ───▶│  cache::load(repo, model)? ──hit──▶ (message-only call if null) ──▶ commit ─┼─▶ cache::advance
                 │        │                                                                     │
                 │       miss                                                                   │
                 │        ▼                                                                     │
                 │  groq::generate_plan ─▶ validate_basic ─▶ cache::save ─▶ commit ─────────────┼─▶ cache::advance
                 │                                                                              │
  --reset ───────┼─▶ cache::clear (up front)                                                    │
  --all / fallback┼─▶ cache::clear                                                              │
                 └──────────────────────────────────────────────────────────────────────────┘
```

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/cache.rs` | **New** | Cache path (sha256(repo-root) in OS cache dir, 0600), load+freshness check, save, advance, clear, fingerprint. |
| `src/plan.rs` | Modified | Derive `Serialize` on `Plan`/`Group` (currently `Deserialize` only) so the plan round-trips through the cache; add a small content-hash helper if it lives here. |
| `src/main.rs` | Modified | Wire cache load/save/advance into `execute`/`commit_first_group`; `commit_group_flow` returns `CommitOutcome { Committed, Aborted }` and `cache::advance` is gated on `Committed` (abort/dry-run never advance); narrow the restore-on-error wrapper (Q4); clear cache on `--reset`/`--all`/fallback. |
| `src/groq.rs` | Modified | Add `resolved_model()` (model string without requiring the API key) for the fingerprint; reuse `generate_commit_message` for the per-group message. |
| `src/diff.rs` | Modified | Add a file-scoped gather (`gather_for_files`) so the per-group message is generated against only that group's diff (FR-45); **refactor `append_untracked` to filter to an allow-list of paths** (it currently appends every untracked file in the repo - reused unfiltered it would pollute a single group's message prompt). |
| `src/git.rs` | Modified | **`commit_signed` returns the new `GcmError::CommitFailed`** (not `GcmError::Git`) on a non-zero `git commit`, and drops the now-false "index restored" wording (the group is left staged on that path). `clear_staged`/`stage_group` keep returning `GcmError::Git` so the failure discriminator is unambiguous (only the commit step yields `CommitFailed`). |
| `src/error.rs` | Modified | Add `GcmError::CommitFailed(String)` (carries the git error for FR-58 "surface the error") and `leaves_staged(&self) -> bool` (`true` only for `CommitFailed`). |
| `src/cli.rs` | Modified | Add the `--reset` flag (FR-8). |
| `Cargo.toml` | Modified | Add `directories` (OS cache dir, ADR-001 #12) and `sha2` (key + content hashes). |

### Dependencies

- **Internal**: `git::Repo` (`root()`, `has_head()`, `changed_files()`, `commit_signed()`),
  `plan::{Plan, Group, validate_basic}`, `groq::{generate_commit_message, resolved_model}`,
  `diff::gather_for_files`.
- **External (new)**: `directories` (cross-platform cache directory, ADR-001 #12);
  `sha2` (SHA-256 for the repo-root key and the content fingerprint). No `hex` crate -
  hex-encode the digest with a small local helper to keep the dependency surface minimal.

---

## Detailed Design

### Cache file format

The on-disk file is a JSON wrapper around the typed `Plan`. FR-30 (reading the bash
`{groups:[...]}` format) was **dropped** by ADR-001 #12, so the format is free to add a
fingerprint envelope; a one-time cold re-analysis on cutover is acceptable.

```rust
// src/cache.rs
#[derive(Serialize, Deserialize)]
struct CacheFile {
    /// Cache *file-format* version. Bumped only when this struct's shape changes;
    /// on read, a mismatch is treated as a cache miss (the file is ignored/replaced).
    version: u32,                 // = CACHE_FORMAT_VERSION (1)
    /// Hex SHA-256 over the pending-file content + provider/model + prompt/schema
    /// version (see Freshness fingerprint). Recomputed every time the cache advances.
    fingerprint: String,
    /// The remaining (not-yet-committed) groups, verbatim from `generate_plan`.
    plan: Plan,
}
```

`Plan`/`Group` gain `#[derive(Serialize)]`. A later group's `commit_message: null`
serializes faithfully and is regenerated at commit time.

### Cache location & key (FR-25, FR-29)

- **Key**: `sha256(repo-root-absolute-path)`, hex. The repo root comes from
  `Repo::root()`. (The bash tool truncated to 16 hex chars; we keep the full digest -
  collision-free and the length is irrelevant in a per-user cache dir.)
- **Location**: `directories::ProjectDirs::from("", "", "gcm").cache_dir()`, i.e.
  `~/Library/Caches/gcm/` (macOS), `~/.cache/gcm/` (XDG/Linux),
  `%LOCALAPPDATA%\gcm\cache\` (Windows). **Not** a hardcoded `/tmp` path (the bash bug,
  FR-29). File name: `plan-<key>.json`.
- **Permissions**: atomic write = create a temp file in the same dir, **`chmod 0o600` on it
  before writing the plan**, then rename over the target (`#[cfg(unix)]`). Setting the mode
  *before* the content avoids a window where the plan is world-readable. On Windows the
  per-user cache dir ACL already restricts access; document this.
- **Provider-agnostic**: the key is the repo, not the provider, so `gcm`/`gcmq`/`gcmc`
  share one plan for a repo (PRD FR-25). The provider/model is folded into the *fingerprint*,
  so switching models re-analyzes rather than reusing a foreign plan. The fingerprint's
  provider token is the literal `"groq:"` while Groq is the only backend; once the provider
  trait lands (CLO-489) this must become the *active* provider's id so a provider switch
  re-analyzes. Noted so it is not left hardcoded.

- **FR-30 (read the bash `{groups:[...]}` cache) is explicitly *covered = dropped*** per
  ADR-001 #12, not implemented: a one-time cold re-analysis on personal cutover is
  acceptable, and the new `version`/`fingerprint` envelope is incompatible with the bash
  shape by design.

### Freshness fingerprint (FR-27)

The fingerprint answers "is the cached plan still valid for the current working tree?"
without pinning `HEAD` (pinning HEAD would self-invalidate the remainder after every
commit and defeat FR-26).

```
fingerprint = hex( sha256(
    FINGERPRINT_VERSION ‖ 0x00 ‖
    provider_model      ‖ 0x00 ‖              // e.g. "groq:openai/gpt-oss-120b"
    for each pending path P, sorted:          // pending = current `git status` paths
        P ‖ 0x00 ‖ content_hash(P) ‖ 0x00
) )

content_hash(P) = sha256(working-tree bytes of P)   if P exists in the working tree
                = "\0DELETED"                         if P is a pending deletion
```

> **Memory: hash by streaming, never `fs::read`.** `content_hash` must hash the file
> with **constant memory** - open the file, wrap in a `BufReader`, and
> `std::io::copy(&mut reader, &mut Sha256::new())` (sha2's `Sha256` implements
> `io::Write`). A naive `std::fs::read(P)` would load entire pending files into memory and
> OOM on a large binary/data file that is still in `git status` before a `.gitignore`
> catches it. This matches the existing `diff.rs` invariant ("a single huge file is never
> loaded into memory in full", `read_capped`). Unlike the *prompt* diff (capped at 8 KB for
> the token budget), the fingerprint hashes the **full** content - a content change past
> 8 KB must still flip the fingerprint - so the fix is streaming, not capping.

- **Pending set is read from LIVE git status each run**, not from the cached plan. So a
  changed file set (file added/removed from the change set) produces a different
  fingerprint - the bash name-set check is *subsumed* by the digest.
- **Content, not names**: an edit to a pending file changes its content hash -> mismatch ->
  re-analyze. This is the bug fix (the bash tool compared names only).
- **No HEAD pin**: after committing group 1, its files leave `git status`; the remaining
  files' content is unchanged, so the re-stamped fingerprint (computed over the remaining
  set) still matches on the next run - caching survives the commit (FR-26).
- **Provider/model + prompt/schema version** are in the digest: switching `GCM_GROQ_MODEL`
  or changing `GROUPING_SYSTEM_PROMPT`/`plan::schema()` (bump `FINGERPRINT_VERSION`) forces
  re-analysis.
- **Unborn-branch safe**: reads working-tree bytes and `git status` only - no `HEAD`
  required (CLO-487 already made `changed_files()` unborn-safe).
- **Renames**: the pending path is the NEW path (consistent with `generate_plan`'s prompt
  rule and CLO-487 staging); its working-tree content is hashed.

### Message on a cache hit (FR-45, ADR-001 #6)

On a cache **hit**, no grouping call is made. The now-first group's message is resolved as:

- If `cached.plan.groups[0].commit_message` is present and non-empty (a *full* plan cached
  from a fresh grouping, e.g. saved by a prior `--dry-run`): use it as-is. **Zero LLM
  calls.**
- If it is `null` (an *advanced* plan - this group was originally group 2+): make a
  **message-only** call `groq::generate_commit_message(diff_of_group0)` scoped to that
  group's files via `diff::gather_for_files`, and fill `commit_message`. **One cheap
  message call; still no grouping call.** The diff is the **working-tree diff of group 0's
  files taken *before* staging** (so it reflects exactly what the upcoming commit will
  contain, independent of `clear_staged`/`stage_group`); the message call therefore runs
  before `commit_group_flow` stages anything.

This is the deliberate reading of the Linear AC "group 2 commits from cache with **no new
analysis call**": *analysis* = the grouping call, which is skipped. A scoped message-only
call is the ADR-001 #6 regenerate-per-group mechanism and is expected. The test for AC-1
asserts the **grouping** endpoint is not hit (a message call may be). See
[Open Questions](#open-questions) - this interpretation is flagged for explicit sign-off.

### Commit-safe advancement (FR-26, FR-58)

Today `main.rs::commit_first_group` snapshots the index and restores it on **any** error
from `commit_group_flow` (FR-47). FR-58 requires the opposite for the commit step: a
rejected `git commit` must **leave the group staged**. The flow is narrowed so the two
rules do not collide:

```
commit_group_flow -> Result<CommitOutcome, GcmError>     // CommitOutcome = Committed | Aborted
    match confirm:
        Abort  -> return Ok(Aborted)         // nothing staged, no restore, NO advance
    // Commit:
    repo.clear_staged()?                     // failure here -> restore index (FR-47)
    repo.stage_group(group0_files)?          // failure here -> restore index (FR-47)
    repo.commit_signed(message)              // failure here -> DO NOT restore (FR-58):
        Err -> GcmError::CommitFailed(..)    //   leave group staged, do not advance, surface error
        Ok  -> return Ok(Committed)

// caller (commit_first_group), AFTER the restore-on-error decision:
if outcome == Committed { cache::advance(repo, plan, model) }   // ONLY on Committed
```

**Advance is gated on `Committed`, never on a bare `Ok(())`.** Today `commit_group_flow`
returns `Ok(())` for *both* a successful commit and a user `Abort` (and `--dry-run` also
returns `Ok`). If `cache::advance` were wired to "`commit_first_group` returned `Ok`", an
**abort would drop group 0 and the next run would permanently skip it**. The new
`CommitOutcome { Committed, Aborted }` return makes the gate explicit and un-mis-wireable;
`advance` runs only on `Committed`. (`--dry-run` returns before this flow entirely - it
never advances, FR-7.)

`commit_first_group`'s wrapper restores the snapshot only when the error does **not** leave

exec
/bin/zsh -lc 'git diff --stat main...HEAD' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
 Cargo.lock                                        | 139 ++++++
 Cargo.toml                                        |   2 +
 docs/DEPENDENCIES.md                              |   3 +-
 docs/PROJECT.md                                   |   9 +-
 docs/ROADMAP.md                                   |   4 +-
 docs/design-docs/2026-06-20-clo-491-plan-cache.md | 551 ++++++++++++++++++++++
 docs/reviews/clo-491-review-claude-fallback.md    | 248 ++++++++++
 docs/reviews/clo-491-review-synthesis.md          |  71 +++
 docs/status/clo-491-workflow.yaml                 | 169 +++++++
 scripts/acceptance.sh                             | 189 +++++++-
 src/cache.rs                                      | 433 +++++++++++++++++
 src/cli.rs                                        |   4 +
 src/diff.rs                                       |  47 +-
 src/error.rs                                      |  44 ++
 src/git.rs                                        |  63 ++-
 src/groq.rs                                       |  15 +-
 src/main.rs                                       | 100 +++-
 src/plan.rs                                       |   8 +-
 18 files changed, 2046 insertions(+), 53 deletions(-)

exec
/bin/zsh -lc 'git diff --name-only main...HEAD' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
Cargo.lock
Cargo.toml
docs/DEPENDENCIES.md
docs/PROJECT.md
docs/ROADMAP.md
docs/design-docs/2026-06-20-clo-491-plan-cache.md
docs/reviews/clo-491-review-claude-fallback.md
docs/reviews/clo-491-review-synthesis.md
docs/status/clo-491-workflow.yaml
scripts/acceptance.sh
src/cache.rs
src/cli.rs
src/diff.rs
src/error.rs
src/git.rs
src/groq.rs
src/main.rs
src/plan.rs

codex
I have the design summary and task mapping. Next I’m reading the full touched Rust modules plus the acceptance script so I can check the actual behavior against the intended failure and cache semantics.
exec
/bin/zsh -lc "sed -n '261,620p' docs/design-docs/2026-06-20-clo-491-plan-cache.md" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
the group staged:

```rust
let result = commit_group_flow(..);
if let Err(e) = &result {
    if !e.leaves_staged() {            // CommitFailed -> true (leave staged); others -> restore
        let _ = repo.restore_index(&snapshot);
    }
}
```

This requires a matching change in `git.rs::commit_signed` itself: today (git.rs:161-175) a
non-zero `git commit` returns `GcmError::Git("git commit failed ...; index restored")`. That
variant makes `leaves_staged()` `false` (the wrapper would restore and unstage the group,
failing AC-3) and the "index restored" wording is now wrong. So `commit_signed` returns the
new `GcmError::CommitFailed(<git stderr context>)` and drops that wording. Crucially,
`clear_staged` and `stage_group` keep returning `GcmError::Git`, so the discriminator is
purely by variant - only the commit step produces `CommitFailed`, no string matching needed.
`GcmError::leaves_staged()` returns `true` for `CommitFailed` and carries the underlying git
error to the user (FR-58 "surface the error"). A pre-commit hook that *reformats and
re-stages* the committed group is acceptable - the commit then succeeds and the cache
advances normally; nothing special is required for that case.

**Accepted tradeoff - index selection on commit failure.** `commit_group_flow` runs
`clear_staged` (`read-tree HEAD`) *before* staging group 0, which resets the **index** to
`HEAD` and discards any pre-existing manual staging selection (e.g. `git add -p` hunks).
Because the commit-failure path deliberately does **not** restore the snapshot (FR-58 leaves
group 0 staged), the user is left with only group 0 in the index. This loses the *staging
selection only* - never working-tree changes (`clear_staged`/`restore_index` touch the index,
not the work tree). It is consistent with the success path (which already does not restore)
and with the existing `cli.rs` `EGRESS_DISCLOSURE` ("overrides any manual hunk-level
`git add -p` staging"). Documented as an accepted property of gcm owning the index, not a
bug to fix.

**Advance** (`cache::advance`): on commit success, drop `groups[0]`; if `groups[1..]` is
empty, delete the cache file; otherwise recompute the fingerprint over the new pending set
and write `{version, fingerprint, plan: {groups: groups[1..]}}`. The advance happens after
the commit, when group 0's files have left the working tree, so the recomputed fingerprint
covers exactly the still-pending files.

### Cache lifecycle in `execute`

| Trigger | Cache action |
|---|---|
| `--reset` (FR-8, FR-28) | `cache::clear` **up front**, then forced miss -> `generate_plan`. |
| `--all` or merge-in-progress | `cache::clear`, then single-commit path (no grouping). |
| Grouping fallback (parse/validation failure, FR-28) | `cache::clear`, then single-commit path. |
| Cache **miss** (no/stale/corrupt file) | `generate_plan` -> `validate_basic` -> `cache::save` (full plan, fingerprint over all pending) -> commit -> advance. |
| Cache **hit** (fresh) | use cached plan -> resolve message (above) -> commit -> advance. |
| `--dry-run` (FR-7) | uses/saves the plan but **does not advance** (preview only). A miss still `save`s the full plan so a following real run hits it. |

### Resilience: the cache never blocks a commit

Cache I/O is best-effort. A corrupt/unreadable file is treated as a **miss** (re-analyze);
a failed `save`/`advance` write **warns and continues** (the commit already succeeded). If
an advance write fails after a successful commit, the stale cache self-heals on the next
run: the just-committed files have left `git status`, so the live fingerprint no longer
matches the stored one -> miss -> re-analyze. A cache problem must never abort a commit
(that path is reserved for FR-58 commit failures).

### Code Structure

```rust
// src/cache.rs  (new)
pub fn load(repo: &Repo, model: &str) -> Option<Plan>;     // Some(plan) only if fresh
pub fn save(repo: &Repo, plan: &Plan, model: &str);        // best-effort; warns on failure
pub fn advance(repo: &Repo, plan: &Plan, model: &str);     // drop group0; delete if empty; re-stamp
pub fn clear(repo: &Repo);                                 // delete file (--reset/--all/fallback)

fn cache_path(repo_root: &Path) -> Option<PathBuf>;        // ProjectDirs + sha256(root)
fn fingerprint(repo: &Repo, pending: &[ChangedFile], model: &str) -> String;
fn content_hash(repo: &Repo, file: &ChangedFile) -> String;
```

### API / Interface Design

| Function | Parameters | Returns | Description |
|----------|------------|---------|-------------|
| `cache::load` | `&Repo, &str` | `Option<Plan>` | Read+deserialize, recompute fingerprint over live pending, return `Some` iff fresh. |
| `cache::save` | `&Repo, &Plan, &str` | `()` | Atomic 0600 write of the full plan with a fresh fingerprint; best-effort. |
| `cache::advance` | `&Repo, &Plan, &str` | `()` | Drop group 0; delete file if no groups remain, else re-stamp+write; best-effort. |
| `cache::clear` | `&Repo` | `()` | Remove the cache file (ignore "not found"). |
| `groq::resolved_model` | – | `String` | Model id from `GCM_GROQ_MODEL` or default, without requiring `GROQ_API_KEY`. |
| `diff::gather_for_files` | `&Repo, &[&ChangedFile]` | `Result<GatheredDiff, GcmError>` | Diff + stat restricted to the given paths (tracked pathspec diff + untracked content **filtered to those paths** + unborn case), for the per-group message. |
| `GcmError::leaves_staged` | `&self` | `bool` | `true` for `CommitFailed` (FR-58 leave staged), else `false`. |
| `commit_group_flow` (main) | `…` | `Result<CommitOutcome, GcmError>` | `CommitOutcome::{Committed, Aborted}`; `cache::advance` runs only on `Committed`. |

---

## Implementation Plan

### Phase 1: Dependencies & cache module

- [ ] Add `directories` and `sha2` to `Cargo.toml`
- [ ] `src/cache.rs`: `cache_path` (ProjectDirs + sha256(repo-root) hex), dir creation
- [ ] `fingerprint` + `content_hash` (working-tree bytes / `\0DELETED`; provider-model; version), `FINGERPRINT_VERSION`, `CACHE_FORMAT_VERSION`
- [ ] `CacheFile` struct; derive `Serialize` on `plan::{Plan, Group}`
- [ ] `load` (deserialize + format-version check + fingerprint match), `save` (atomic temp+rename, 0600), `advance`, `clear`

### Phase 2: Provider/diff helpers

- [ ] `groq::resolved_model()` (no key required)
- [ ] `diff::gather_for_files(repo, &[&ChangedFile])` - genuinely new (the existing
  `diff_full`/`diff_stat` take no pathspec). It must assemble, for the given paths only:
  (a) the path-scoped **tracked** diff (`git diff [-- HEAD] -- <paths>`, NUL-literal pathspecs,
  `core.quotePath=false`), (b) the **untracked** files' content **filtered to the group's
  paths** (see next item), and (c) the **unborn-branch** case (diff against the empty tree /
  `--cached`, no `HEAD`). This is the largest new surface in the task - budget for it in the
  plan as its own sub-task with unit coverage.
- [ ] **Refactor `diff::append_untracked` to take an allow-list of paths** (or filter
  `repo.untracked_files()` before appending). Today it appends **every** untracked file in the
  repo (diff.rs:100-101); reused as-is for a single group it would pollute group 0's message
  prompt with other groups' untracked files and hallucinate the message. `gather`/
  `gather_for_grouping` pass "all" (unchanged behavior); `gather_for_files` passes the group's
  paths. **Unborn-branch note**: on an unborn branch nothing is tracked, so the path-scoped
  tracked diff is empty and *all* of group 0's content arrives through this filtered untracked
  path - the filter must be exact (eval row 18). (In practice the *message-only* call never
  fires while still unborn - committing the first group creates `HEAD` before any cache hit on
  an advanced group - but `gather_for_files` must be correct for the untracked-only case
  regardless.)

### Phase 3: Wire into the commit flow + failure semantics

- [ ] `cli.rs`: add `--reset`
- [ ] `error.rs`: `GcmError::CommitFailed` + `leaves_staged()`
- [ ] `main.rs`: `--reset`/`--all`/fallback clear the cache; cache `load` before `generate_plan`; `save` after validate; resolve the per-group message on a hit (null -> message-only call); `advance` on commit success; narrow the restore-on-error wrapper (leave staged on `CommitFailed`)
- [ ] `main.rs`: `--dry-run` saves-but-does-not-advance

### Phase 4: Testing & validation

- [ ] Unit tests: fingerprint stability/sensitivity, advance/delete, format-version mismatch, content vs name change, `leaves_staged()`
- [ ] `scripts/acceptance.sh`: the 4 Linear ACs + edge cases (below) against the mock-Groq harness
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test` / release build all green
- [ ] Dual-model validation gate (Gemini + Codex, read-only), same pattern as CLO-487

---

## Constraints

**Must**:
- Cache key = `sha256(repo-root)`; location = OS cache dir via `directories` (never a
  hardcoded `/tmp`); file mode `0600` on Unix (FR-25, FR-29).
- Freshness = content fingerprint over pending files, recomputed on advance, **never**
  pinning `HEAD`; unborn-branch safe (FR-27).
- Regenerate-per-group messages (ADR-001 #6, FR-45): a cache hit makes **no grouping
  call**; only `groups[0]` ever carries a message from the initial plan.
- On `git commit` failure: do not advance the cache, leave the group staged, surface the
  error (FR-58, FR-26).
- `--reset` clears the cache before running; `--all` and any fallback clear it (FR-8, FR-28).
- Path handling stays consistent with CLO-487 (`-uall` NUL status, literal NUL-stdin
  staging, rename = new path) - the cache reuses `changed_files()` output, it does not
  re-parse status.

**Must-not**:
- Must not pin analysis-time `HEAD` in the fingerprint (would defeat FR-26).
- Must not advance the cache on a user **abort** or `--dry-run` - advance is gated on
  `CommitOutcome::Committed`, not on a bare `Ok(())` (else a group is silently skipped).
- Must not load whole pending files into memory to fingerprint them - the content hash
  **streams** (`BufReader` + `io::copy` into the hasher), constant memory.
- Must not let any cache I/O failure abort a commit - cache is best-effort; corrupt/missing
  -> re-analyze, write failure -> warn and continue.
- Must not write API keys, diffs, or message content anywhere except the plan the LLM
  already returned (no secrets in the cache; PRD Security NFR).
- Must not restore the index after a `commit_signed` failure (that would unstage the group,
  violating FR-58).
- Must not feed un-filtered untracked files into a single group's message diff - scope
  `append_untracked` to the group's paths (else the message hallucinates over other groups).

**Prefer**:
- Prefer reusing existing primitives (`generate_commit_message`, `changed_files`,
  `snapshot_index`/`restore_index`) over new machinery.
- Prefer hand-rolled hex encoding over adding the `hex` crate (minimal dependency surface).
- Prefer atomic writes (temp + rename) over in-place truncation.

**Escalate when**:
- The "no new analysis call" AC is interpreted as **zero** LLM calls (would contradict
  ADR-001 #6 regenerate-per-group) - confirm the message-only interpretation before
  implementing (see Open Questions).
- Honoring FR-58 would require changing `commit_signed`'s signature or the
  snapshot/restore contract beyond narrowing the wrapper.
- A cross-platform perms model beyond `0600`-on-Unix + cache-dir-ACL-on-Windows is needed.

---

## Acceptance Criteria

- [ ] **AC-1** (FR-2, FR-26, FR-45): commit group 1, re-run -> group 2 commits **from cache**
  with a valid message and **no grouping call**. Verify in `scripts/acceptance.sh` against
  the mock-Groq harness: the second run records zero `response_format`/grouping requests
  (a message-only request is allowed) and produces a non-empty conventional message.
- [ ] **AC-2** (FR-27): editing a pending file between runs invalidates the cache and
  re-analyzes. Acceptance test: run 1 (group 1 commits), edit a still-pending file, run 2
  records a fresh grouping call (fingerprint mismatch).
- [ ] **AC-3** (FR-58, FR-26): a failing pre-commit hook leaves the plan un-advanced and the
  group still staged. Acceptance test with a hook that `exit 1`s: run exits non-zero,
  `git diff --cached --name-only` lists group 1's files, the cache file is unchanged
  (still holds the un-advanced plan), and the next run retries the same group.
- [ ] **AC-4** (FR-31 unborn): the first commit in a brand-new repo (no `HEAD`) works end to
  end with the cache (save on miss, advance on success).
- [ ] **AC-5** (FR-29): the cache file lives under the OS cache dir (not `/tmp`) and is mode
  `0600` on Unix. Verify: `stat` the file path returned for a test repo.
- [ ] **AC-6** (FR-8, FR-28): `--reset` deletes the cache and forces a grouping call; `--all`
  and a grouping fallback both clear the cache.
- [ ] **AC-7** (FR-26 abort-safety): aborting at the confirmation prompt (answer `n`, or a
  non-committing `--dry-run`) leaves the cache **un-advanced**. Acceptance test: warm the
  cache, abort, assert the cache file is byte-identical and the next run offers the same
  group 0.

**Verification method**: `cargo fmt --check && cargo clippy --all-targets -- -D warnings &&
cargo test && ./scripts/acceptance.sh` - all green; the new acceptance cases AC-1..AC-6
pass.

---

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | Cache hit advances to group 2, no grouping call | group 2 committed; mock records 0 grouping calls, 1 message call; valid message | acceptance: 2-group change, commit g1, re-run |
| 2 | Full-plan hit after dry-run needs zero LLM calls | real run commits g1 with the cached message; 0 LLM calls on the real run | acceptance: `--dry-run` then real run, same tree |
| 3 | Edit a pending file -> re-analyze | run 2 records a fresh grouping call | acceptance: commit g1, edit a g2 file, re-run |
| 4 | Rename a pending file -> re-analyze | fingerprint mismatch -> grouping call | acceptance: commit g1, `git mv` a g2 file, re-run |
| 5 | Pre-commit hook rejects -> un-advanced, staged | exit!=0; g1 staged; cache file byte-identical; next run retries g1 | acceptance: install `exit 1` hook |
| 6 | Hook reformats+re-stages -> commit succeeds, advance | commit lands; cache advances to g2 | acceptance: hook that edits+`git add`+exit 0 |
| 7 | Unborn-branch first commit with cache | g1 commits; cache saved then advanced; no HEAD errors | acceptance: fresh `git init`, 2-group tree |
| 8 | Cache file perms + location | path under OS cache dir; mode 0600 (Unix) | unit/acceptance: assert `cache_path` parent + `stat -f %Lp` |
| 9 | `--reset` forces re-analysis | cache deleted up front; grouping call made | acceptance: warm cache, run `--reset` |
| 10 | `--all` / fallback clears cache | cache file absent after run | acceptance: warm cache, run `--all`; malformed-plan fallback |
| 11 | Single-group plan -> cache deleted after commit | no cache file remains (nothing to advance to) | acceptance: 1-group change, commit |
| 12 | Corrupt cache file -> treated as miss | re-analyzes; no panic | unit: write garbage to the cache path, `load` returns `None` |
| 13 | Format-version bump -> miss | old-version file ignored, re-analyzes | unit: write `version: 0`, `load` returns `None` |
| 14 | Provider/model change -> re-analyze | switching `GCM_GROQ_MODEL` mismatches the fingerprint | unit: fingerprint differs across model strings |
| 15 | Advance-write failure self-heals | next run re-analyzes (live fingerprint != stored) | reasoned/unit: simulate via stale fingerprint |
| 16 | Deleted pending file folds into fingerprint | delete marker distinguishes present vs deleted | unit: `content_hash` for a deletion |
| 17 | Group 0 is a deletion-only group | message-only call gets the deletion diff; commit records the removal; cache advances | acceptance: plan whose group 0 deletes a file, cache hit |
| 18 | Group 0 is untracked-only | `gather_for_files` includes only that group's untracked content; message generated; commit adds the new file | acceptance: plan whose group 0 is a new untracked file, cache hit |
| 19 | User **aborts** at the prompt -> cache NOT advanced | cache file byte-identical; next run offers the same group 0 | acceptance: warm cache, answer `n` at confirm |
| 20 | Large pending binary -> no OOM | fingerprint computed with bounded memory; process does not crash | unit/acceptance: a >100 MB pending file, `content_hash` streams |
| 21 | Untracked filter: group 0's message excludes other groups' untracked files | message diff contains only group 0's untracked paths | unit: `gather_for_files` with a multi-group untracked set |

**Edge cases to cover**:
- Provider unset / `GROQ_API_KEY` absent during fingerprinting (use `resolved_model`, which
  does not require the key).
- Cache dir does not yet exist (create it; do not fail).
- Concurrent runs in the same repo (last writer wins; atomic rename prevents a torn file).
- A pending file outside the repo root or with an exotic path (NUL-safe; reuse
  `changed_files()` paths, do not re-parse).

---

## Testing Strategy

- **Unit tests** (`src/cache.rs`, `src/plan.rs`): `fingerprint` stability (same inputs ->
  same digest) and sensitivity (content change, name-set change, model change, version bump
  each flip it); `advance` drops group 0 and deletes on empty; `load` rejects wrong format
  version and corrupt JSON; `content_hash` deletion marker; `GcmError::leaves_staged`.
  TDD: stub -> RED -> impl -> GREEN, with a mutation check proving each test is load-bearing
  (the CLO-487 discipline).
- **Integration / acceptance** (`scripts/acceptance.sh` + mock-Groq harness): AC-1..AC-6 and
  eval rows 1-11, using real `git` in temp repos and the mock provider to count grouping vs
  message calls. Reuse the CLO-487 harness conventions.
- **Manual**: in a scratch repo with a real `GROQ_API_KEY`, make a multi-group change,
  `gcm`, edit a pending file, `gcm` (re-analyzes), `gcm` to finish; install a rejecting hook
  and confirm staged-and-un-advanced behavior.

---

## Open Questions

_All resolved at the design checkpoint (2026-06-20)._

- [x] **"No new analysis call" = no *grouping* call, message-only call allowed.** **RESOLVED
  (owner sign-off):** *analysis* = the grouping call, which a cache hit skips; the
  regenerate-per-group message-only call (ADR-001 #6, FR-45) is expected and cheap. The
  AC-1 test asserts zero grouping requests on the re-run (a message-only request is allowed).
  The strict zero-LLM-calls reading (pre-cache every group's message) was rejected by
  ADR-001 #6 and would be an ADR change, not a CLO-491 tweak.
- [x] **Key truncation**: **RESOLVED:** use the full sha256 hex (no functional difference vs
  the bash 16-char prefix; avoids a contrived collision). FR-30 bash-cache compat is dropped
  (ADR-001 #12), so no external tooling depends on the old file name.

---

## References

- [Linear Task CLO-491](https://linear.app/cloud-ai/issue/CLO-491/add-per-repo-plan-cache-with-commit-safe-advancement)
- [ADR-001 Foundational Architecture Decisions](../adrs/001-foundational-architecture-decisions.md) (#6 regenerate-per-group, #12 cache location)
- [PRD: gcm](../prds/prd-gcm.md) (FR-2/8/25-30/45/58, Data Model: the grouping plan)
- [CLO-487 spec](../specs/2026-06-20-clo-487-semantic-grouping.md) (the grouping slice this extends)
- Bash reference: `docs/tmp/git-commit-ai.sh` (cache: `:65-67`, `:283-297`, `:470-478`)
- Workflow state: `docs/status/clo-491-workflow.yaml`

exec
/bin/zsh -lc "sed -n '1,260p' src/cache.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
//! Per-repo plan cache (CLO-491). Persists the grouping [`Plan`] so re-runs
//! commit the next group without re-calling the grouping LLM (FR-25), advancing
//! one group per successful commit (FR-26). Freshness is a content fingerprint
//! over the pending files - not file names (the bash bug) and never a `HEAD` pin
//! (FR-27). The cache is best-effort: a read failure is a miss (re-analyze), a
//! write failure warns and continues; it never aborts a commit.

use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::GcmError;
use crate::git::{ChangedFile, Repo};
use crate::plan::Plan;

/// On-disk cache file format version. Bumped only when [`CacheFile`]'s shape
/// changes; on read a mismatch is a miss (the stale file is ignored/replaced).
const CACHE_FORMAT_VERSION: u32 = 1;
/// Folded into the fingerprint: bump when the grouping prompt or schema changes
/// so a cached plan from an older contract re-analyzes.
const FINGERPRINT_VERSION: u32 = 1;
/// Provider token in the fingerprint. Groq is the only backend until the
/// provider trait lands (CLO-489), after which this must become the active
/// provider's id so a provider switch re-analyzes.
const PROVIDER: &str = "groq";

/// The JSON wrapper persisted to disk: a fingerprint envelope around the typed
/// plan. (FR-30 bash-cache compat was dropped by ADR-001 #12, so the format is
/// free to carry this envelope.)
#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    fingerprint: String,
    plan: Plan,
}

/// Load the cached plan iff it is fresh for the current working tree. Returns
/// `None` on any miss: no file, wrong format version, corrupt JSON, or a
/// fingerprint mismatch (an edit/rename/added-or-removed file, or a
/// provider/model/prompt change).
pub fn load(repo: &Repo, model: &str) -> Option<Plan> {
    let path = cache_path(repo.root())?;
    let data = fs::read(&path).ok()?;
    let cf = read_cache_file(&data)?;
    let pending = repo.changed_files().ok()?;
    if fingerprint(repo, &pending, model) != cf.fingerprint {
        return None;
    }
    Some(cf.plan)
}

/// Persist the full plan with a fresh fingerprint over the current pending set.
/// Best-effort: a failure warns and returns (the caller's commit still proceeds).
pub fn save(repo: &Repo, plan: &Plan, model: &str) {
    if let Err(e) = persist(repo, plan, model) {
        eprintln!("gcm: warning: could not write plan cache: {e}");
    }
}

/// Advance the cache after a successful commit: drop `groups[0]`. If no groups
/// remain, delete the file; otherwise re-stamp the fingerprint over the new
/// (shrunken) pending set and write. Best-effort - a failure self-heals on the
/// next run (the just-committed files leave `git status`, so the live
/// fingerprint no longer matches the stored one -> miss -> re-analyze).
pub fn advance(repo: &Repo, plan: &Plan, model: &str) {
    if let Err(e) = advance_inner(repo, plan, model) {
        eprintln!("gcm: warning: could not advance plan cache: {e}");
    }
}

/// Delete the cache file (used by `--reset`, `--all`, and the single-commit
/// fallback). A missing file is not an error.
pub fn clear(repo: &Repo) {
    if let Some(path) = cache_path(repo.root()) {
        let _ = fs::remove_file(path);
    }
}

// ── internals ────────────────────────────────────────────────────────────

fn persist(repo: &Repo, plan: &Plan, model: &str) -> io::Result<()> {
    let path = cache_path(repo.root()).ok_or_else(no_cache_dir)?;
    let pending = repo.changed_files().map_err(to_io)?;
    let cf = CacheFile {
        version: CACHE_FORMAT_VERSION,
        fingerprint: fingerprint(repo, &pending, model),
        plan: plan.clone(),
    };
    write_atomic(&path, &serialize(&cf)?)
}

fn advance_inner(repo: &Repo, plan: &Plan, model: &str) -> io::Result<()> {
    let path = cache_path(repo.root()).ok_or_else(no_cache_dir)?;
    let remaining = remaining_groups(plan);
    if remaining.groups.is_empty() {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    let pending = repo.changed_files().map_err(to_io)?;
    let cf = CacheFile {
        version: CACHE_FORMAT_VERSION,
        fingerprint: fingerprint(repo, &pending, model),
        plan: remaining,
    };
    write_atomic(&path, &serialize(&cf)?)
}

/// Parse + validate the on-disk file. `None` for a wrong format version, corrupt
/// JSON, or a structurally-empty plan (a defensive guard - `advance` never
/// writes an empty group 0).
fn read_cache_file(bytes: &[u8]) -> Option<CacheFile> {
    let cf: CacheFile = serde_json::from_slice(bytes).ok()?;
    if cf.version != CACHE_FORMAT_VERSION {
        return None;
    }
    if cf.plan.groups.is_empty() || cf.plan.groups[0].files.is_empty() {
        return None;
    }
    Some(cf)
}

/// The plan with `groups[0]` dropped (pure; the advance unit).
fn remaining_groups(plan: &Plan) -> Plan {
    Plan {
        groups: plan.groups.iter().skip(1).cloned().collect(),
    }
}

/// `<cache_dir>/plan-<sha256(repo-root) hex>.json`. `None` if no cache dir can
/// be determined (e.g. a headless environment with no HOME and no override).
fn cache_path(repo_root: &Path) -> Option<PathBuf> {
    Some(cache_dir()?.join(cache_file_name(repo_root)))
}

/// The cache directory: `GCM_CACHE_DIR` if set (for tests and users who want to
/// relocate it), otherwise the OS cache dir via the `directories` crate
/// (ADR-001 #12, FR-29) - never a hardcoded `/tmp`.
fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("GCM_CACHE_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    ProjectDirs::from("", "", "gcm").map(|d| d.cache_dir().to_path_buf())
}

/// The cache file name for a repo: `plan-<sha256(repo-root) hex>.json` (FR-25
/// key). Pure - directory-independent, so the key/naming is unit-testable.
fn cache_file_name(repo_root: &Path) -> String {
    format!("plan-{}.json", repo_key(repo_root))
}

/// Hex SHA-256 of the absolute repo-root path (FR-25 cache key).
fn repo_key(repo_root: &Path) -> String {
    let mut h = Sha256::new();
    h.update(repo_root.to_string_lossy().as_bytes());
    hex(&h.finalize())
}

/// Fingerprint over the pending change set (FR-27): version + provider/model +
/// per-file (path, content hash), with paths sorted for stability. Read from the
/// LIVE change set each run; never pins `HEAD`; unborn-safe (working-tree reads
/// + `git status` only).
fn fingerprint(repo: &Repo, pending: &[ChangedFile], model: &str) -> String {
    let mut entries: Vec<(String, String)> = pending
        .iter()
        .map(|f| (f.path.clone(), content_hash(repo, f)))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    digest_fingerprint(model, &entries)
}

/// Combine pre-sorted `(path, content_hash)` entries into the fingerprint digest
/// (pure; the fingerprint unit, testable without git or the filesystem).
fn digest_fingerprint(model: &str, entries: &[(String, String)]) -> String {
    let mut h = Sha256::new();
    h.update(FINGERPRINT_VERSION.to_le_bytes());
    h.update(b"\0");
    h.update(PROVIDER.as_bytes());
    h.update(b":");
    h.update(model.as_bytes());
    h.update(b"\0");
    for (path, content) in entries {
        h.update(path.as_bytes());
        h.update(b"\0");
        h.update(content.as_bytes());
        h.update(b"\0");
    }
    hex(&h.finalize())
}

/// SHA-256 of a pending file's working-tree bytes, **streamed** in fixed-size
/// chunks so a large binary still in `git status` cannot OOM the process. A
/// pending deletion (file absent) is a distinct marker so a delete is detected;
/// an unreadable file is its own marker (never silently equal to a real hash).
fn content_hash(repo: &Repo, file: &ChangedFile) -> String {
    let full = repo.root().join(&file.path);
    let f = match fs::File::open(&full) {
        Ok(f) => f,
        Err(_) => return "\0DELETED".to_string(),
    };
    let mut hasher = Sha256::new();
    let mut reader = BufReader::new(f);
    let mut buf = [0u8; 64 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return "\0UNREADABLE".to_string(),
        }
    }
    hex(&hasher.finalize())
}

/// Lowercase hex encoding (avoids pulling in the `hex` crate).
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn serialize(cf: &CacheFile) -> io::Result<Vec<u8>> {
    serde_json::to_vec_pretty(cf).map_err(io::Error::other)
}

/// Atomic write with user-only permissions: write a temp file in the same dir
/// (created `0600` *before* any content lands, so the plan is never briefly
/// world-readable), then rename over the target.
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("cache path has no parent"))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".plan-{}.tmp", std::process::id()));
    {
        let mut f = open_private(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

#[cfg(unix)]
fn open_private(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}


exec
/bin/zsh -lc "sed -n '261,520p' src/cache.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
#[cfg(not(unix))]
fn open_private(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

fn to_io(e: GcmError) -> io::Error {
    io::Error::other(e.to_string())
}

fn no_cache_dir() -> io::Error {
    io::Error::other("no OS cache directory available")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Group, Plan};

    fn group(files: &[&str], msg: Option<&str>) -> Group {
        Group {
            files: files.iter().map(|s| s.to_string()).collect(),
            summary: "s".to_string(),
            commit_message: msg.map(|m| m.to_string()),
        }
    }

    fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(p, h)| (p.to_string(), h.to_string()))
            .collect()
    }

    #[test]
    fn hex_encodes_lowercase_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff, 0xa0]), "000fffa0");
    }

    #[test]
    fn repo_key_is_stable_and_path_specific() {
        let a = repo_key(Path::new("/home/u/repo"));
        let b = repo_key(Path::new("/home/u/repo"));
        let c = repo_key(Path::new("/home/u/other"));
        assert_eq!(a, b, "same path -> same key");
        assert_ne!(a, c, "different path -> different key");
        assert_eq!(a.len(), 64, "full sha256 hex");
    }

    #[test]
    fn cache_file_name_is_plan_prefixed_json() {
        let name = cache_file_name(Path::new("/home/u/repo"));
        assert!(name.starts_with("plan-"), "name: {name}");
        assert!(name.ends_with(".json"), "name: {name}");
        assert!(!name.contains('/'), "single path component, not /tmp/...");
    }

    #[test]
    fn fingerprint_is_stable_for_same_inputs() {
        let e = entries(&[("a.rs", "h1"), ("b.rs", "h2")]);
        assert_eq!(
            digest_fingerprint("groq:m", &e),
            digest_fingerprint("groq:m", &e)
        );
    }

    #[test]
    fn fingerprint_flips_on_content_change() {
        let before = entries(&[("a.rs", "h1")]);
        let after = entries(&[("a.rs", "h2")]); // same name, different content hash
        assert_ne!(
            digest_fingerprint("m", &before),
            digest_fingerprint("m", &after),
            "a content change (not a name change) must invalidate"
        );
    }

    #[test]
    fn fingerprint_flips_on_file_set_change() {
        let one = entries(&[("a.rs", "h1")]);
        let two = entries(&[("a.rs", "h1"), ("b.rs", "h2")]);
        assert_ne!(digest_fingerprint("m", &one), digest_fingerprint("m", &two));
    }

    #[test]
    fn fingerprint_flips_on_model_change() {
        let e = entries(&[("a.rs", "h1")]);
        assert_ne!(
            digest_fingerprint("groq:model-a", &e),
            digest_fingerprint("groq:model-b", &e),
            "switching provider/model must invalidate"
        );
    }

    #[test]
    fn deletion_marker_differs_from_a_real_hash() {
        // A pending deletion must not collide with any content hash.
        let present = entries(&[("a.rs", "deadbeef")]);
        let deleted = entries(&[("a.rs", "\0DELETED")]);
        assert_ne!(
            digest_fingerprint("m", &present),
            digest_fingerprint("m", &deleted)
        );
    }

    #[test]
    fn remaining_groups_drops_the_first() {
        let plan = Plan {
            groups: vec![
                group(&["a.rs"], Some("feat: a")),
                group(&["b.rs"], None),
                group(&["c.rs"], None),
            ],
        };
        let rem = remaining_groups(&plan);
        assert_eq!(rem.groups.len(), 2);
        assert_eq!(rem.groups[0].files, vec!["b.rs"]);
        assert_eq!(rem.groups[1].files, vec!["c.rs"]);
    }

    #[test]
    fn remaining_groups_of_single_group_is_empty() {
        let plan = Plan {
            groups: vec![group(&["a.rs"], Some("feat: a"))],
        };
        assert!(
            remaining_groups(&plan).groups.is_empty(),
            "delete on advance"
        );
    }

    #[test]
    fn read_cache_file_round_trips_a_valid_file() {
        let cf = CacheFile {
            version: CACHE_FORMAT_VERSION,
            fingerprint: "fp".to_string(),
            plan: Plan {
                groups: vec![group(&["a.rs"], Some("feat: a")), group(&["b.rs"], None)],
            },
        };
        let bytes = serialize(&cf).unwrap();
        let back = read_cache_file(&bytes).expect("valid");
        assert_eq!(back.fingerprint, "fp");
        assert_eq!(back.plan.groups.len(), 2);
        assert_eq!(back.plan.groups[1].commit_message, None);
    }

    #[test]
    fn read_cache_file_rejects_wrong_format_version() {
        let json = br#"{"version":0,"fingerprint":"fp","plan":{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"feat: a"}]}}"#;
        assert!(
            read_cache_file(json).is_none(),
            "old format version -> miss"
        );
    }

    #[test]
    fn read_cache_file_rejects_corrupt_json() {
        assert!(read_cache_file(b"not json at all").is_none());
        assert!(read_cache_file(b"").is_none());
    }

    #[test]
    fn read_cache_file_rejects_empty_plan() {
        let json = br#"{"version":1,"fingerprint":"fp","plan":{"groups":[]}}"#;
        assert!(read_cache_file(json).is_none());
        let empty_g0 = br#"{"version":1,"fingerprint":"fp","plan":{"groups":[{"files":[],"summary":"s","commit_message":"m"}]}}"#;
        assert!(read_cache_file(empty_g0).is_none());
    }
}

exec
/bin/zsh -lc 'git diff --unified=80 main...HEAD -- src/cache.rs src/main.rs src/diff.rs src/git.rs src/error.rs src/groq.rs src/cli.rs src/plan.rs' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
diff --git a/src/cache.rs b/src/cache.rs
new file mode 100644
index 0000000..6429980
--- /dev/null
+++ b/src/cache.rs
@@ -0,0 +1,433 @@
+//! Per-repo plan cache (CLO-491). Persists the grouping [`Plan`] so re-runs
+//! commit the next group without re-calling the grouping LLM (FR-25), advancing
+//! one group per successful commit (FR-26). Freshness is a content fingerprint
+//! over the pending files - not file names (the bash bug) and never a `HEAD` pin
+//! (FR-27). The cache is best-effort: a read failure is a miss (re-analyze), a
+//! write failure warns and continues; it never aborts a commit.
+
+use std::fs;
+use std::io::{self, BufReader, Read, Write};
+use std::path::{Path, PathBuf};
+
+use directories::ProjectDirs;
+use serde::{Deserialize, Serialize};
+use sha2::{Digest, Sha256};
+
+use crate::error::GcmError;
+use crate::git::{ChangedFile, Repo};
+use crate::plan::Plan;
+
+/// On-disk cache file format version. Bumped only when [`CacheFile`]'s shape
+/// changes; on read a mismatch is a miss (the stale file is ignored/replaced).
+const CACHE_FORMAT_VERSION: u32 = 1;
+/// Folded into the fingerprint: bump when the grouping prompt or schema changes
+/// so a cached plan from an older contract re-analyzes.
+const FINGERPRINT_VERSION: u32 = 1;
+/// Provider token in the fingerprint. Groq is the only backend until the
+/// provider trait lands (CLO-489), after which this must become the active
+/// provider's id so a provider switch re-analyzes.
+const PROVIDER: &str = "groq";
+
+/// The JSON wrapper persisted to disk: a fingerprint envelope around the typed
+/// plan. (FR-30 bash-cache compat was dropped by ADR-001 #12, so the format is
+/// free to carry this envelope.)
+#[derive(Serialize, Deserialize)]
+struct CacheFile {
+    version: u32,
+    fingerprint: String,
+    plan: Plan,
+}
+
+/// Load the cached plan iff it is fresh for the current working tree. Returns
+/// `None` on any miss: no file, wrong format version, corrupt JSON, or a
+/// fingerprint mismatch (an edit/rename/added-or-removed file, or a
+/// provider/model/prompt change).
+pub fn load(repo: &Repo, model: &str) -> Option<Plan> {
+    let path = cache_path(repo.root())?;
+    let data = fs::read(&path).ok()?;
+    let cf = read_cache_file(&data)?;
+    let pending = repo.changed_files().ok()?;
+    if fingerprint(repo, &pending, model) != cf.fingerprint {
+        return None;
+    }
+    Some(cf.plan)
+}
+
+/// Persist the full plan with a fresh fingerprint over the current pending set.
+/// Best-effort: a failure warns and returns (the caller's commit still proceeds).
+pub fn save(repo: &Repo, plan: &Plan, model: &str) {
+    if let Err(e) = persist(repo, plan, model) {
+        eprintln!("gcm: warning: could not write plan cache: {e}");
+    }
+}
+
+/// Advance the cache after a successful commit: drop `groups[0]`. If no groups
+/// remain, delete the file; otherwise re-stamp the fingerprint over the new
+/// (shrunken) pending set and write. Best-effort - a failure self-heals on the
+/// next run (the just-committed files leave `git status`, so the live
+/// fingerprint no longer matches the stored one -> miss -> re-analyze).
+pub fn advance(repo: &Repo, plan: &Plan, model: &str) {
+    if let Err(e) = advance_inner(repo, plan, model) {
+        eprintln!("gcm: warning: could not advance plan cache: {e}");
+    }
+}
+
+/// Delete the cache file (used by `--reset`, `--all`, and the single-commit
+/// fallback). A missing file is not an error.
+pub fn clear(repo: &Repo) {
+    if let Some(path) = cache_path(repo.root()) {
+        let _ = fs::remove_file(path);
+    }
+}
+
+// ── internals ────────────────────────────────────────────────────────────
+
+fn persist(repo: &Repo, plan: &Plan, model: &str) -> io::Result<()> {
+    let path = cache_path(repo.root()).ok_or_else(no_cache_dir)?;
+    let pending = repo.changed_files().map_err(to_io)?;
+    let cf = CacheFile {
+        version: CACHE_FORMAT_VERSION,
+        fingerprint: fingerprint(repo, &pending, model),
+        plan: plan.clone(),
+    };
+    write_atomic(&path, &serialize(&cf)?)
+}
+
+fn advance_inner(repo: &Repo, plan: &Plan, model: &str) -> io::Result<()> {
+    let path = cache_path(repo.root()).ok_or_else(no_cache_dir)?;
+    let remaining = remaining_groups(plan);
+    if remaining.groups.is_empty() {
+        let _ = fs::remove_file(&path);
+        return Ok(());
+    }
+    let pending = repo.changed_files().map_err(to_io)?;
+    let cf = CacheFile {
+        version: CACHE_FORMAT_VERSION,
+        fingerprint: fingerprint(repo, &pending, model),
+        plan: remaining,
+    };
+    write_atomic(&path, &serialize(&cf)?)
+}
+
+/// Parse + validate the on-disk file. `None` for a wrong format version, corrupt
+/// JSON, or a structurally-empty plan (a defensive guard - `advance` never
+/// writes an empty group 0).
+fn read_cache_file(bytes: &[u8]) -> Option<CacheFile> {
+    let cf: CacheFile = serde_json::from_slice(bytes).ok()?;
+    if cf.version != CACHE_FORMAT_VERSION {
+        return None;
+    }
+    if cf.plan.groups.is_empty() || cf.plan.groups[0].files.is_empty() {
+        return None;
+    }
+    Some(cf)
+}
+
+/// The plan with `groups[0]` dropped (pure; the advance unit).
+fn remaining_groups(plan: &Plan) -> Plan {
+    Plan {
+        groups: plan.groups.iter().skip(1).cloned().collect(),
+    }
+}
+
+/// `<cache_dir>/plan-<sha256(repo-root) hex>.json`. `None` if no cache dir can
+/// be determined (e.g. a headless environment with no HOME and no override).
+fn cache_path(repo_root: &Path) -> Option<PathBuf> {
+    Some(cache_dir()?.join(cache_file_name(repo_root)))
+}
+
+/// The cache directory: `GCM_CACHE_DIR` if set (for tests and users who want to
+/// relocate it), otherwise the OS cache dir via the `directories` crate
+/// (ADR-001 #12, FR-29) - never a hardcoded `/tmp`.
+fn cache_dir() -> Option<PathBuf> {
+    if let Some(dir) = std::env::var_os("GCM_CACHE_DIR") {
+        if !dir.is_empty() {
+            return Some(PathBuf::from(dir));
+        }
+    }
+    ProjectDirs::from("", "", "gcm").map(|d| d.cache_dir().to_path_buf())
+}
+
+/// The cache file name for a repo: `plan-<sha256(repo-root) hex>.json` (FR-25
+/// key). Pure - directory-independent, so the key/naming is unit-testable.
+fn cache_file_name(repo_root: &Path) -> String {
+    format!("plan-{}.json", repo_key(repo_root))
+}
+
+/// Hex SHA-256 of the absolute repo-root path (FR-25 cache key).
+fn repo_key(repo_root: &Path) -> String {
+    let mut h = Sha256::new();
+    h.update(repo_root.to_string_lossy().as_bytes());
+    hex(&h.finalize())
+}
+
+/// Fingerprint over the pending change set (FR-27): version + provider/model +
+/// per-file (path, content hash), with paths sorted for stability. Read from the
+/// LIVE change set each run; never pins `HEAD`; unborn-safe (working-tree reads
+/// + `git status` only).
+fn fingerprint(repo: &Repo, pending: &[ChangedFile], model: &str) -> String {
+    let mut entries: Vec<(String, String)> = pending
+        .iter()
+        .map(|f| (f.path.clone(), content_hash(repo, f)))
+        .collect();
+    entries.sort_by(|a, b| a.0.cmp(&b.0));
+    digest_fingerprint(model, &entries)
+}
+
+/// Combine pre-sorted `(path, content_hash)` entries into the fingerprint digest
+/// (pure; the fingerprint unit, testable without git or the filesystem).
+fn digest_fingerprint(model: &str, entries: &[(String, String)]) -> String {
+    let mut h = Sha256::new();
+    h.update(FINGERPRINT_VERSION.to_le_bytes());
+    h.update(b"\0");
+    h.update(PROVIDER.as_bytes());
+    h.update(b":");
+    h.update(model.as_bytes());
+    h.update(b"\0");
+    for (path, content) in entries {
+        h.update(path.as_bytes());
+        h.update(b"\0");
+        h.update(content.as_bytes());
+        h.update(b"\0");
+    }
+    hex(&h.finalize())
+}
+
+/// SHA-256 of a pending file's working-tree bytes, **streamed** in fixed-size
+/// chunks so a large binary still in `git status` cannot OOM the process. A
+/// pending deletion (file absent) is a distinct marker so a delete is detected;
+/// an unreadable file is its own marker (never silently equal to a real hash).
+fn content_hash(repo: &Repo, file: &ChangedFile) -> String {
+    let full = repo.root().join(&file.path);
+    let f = match fs::File::open(&full) {
+        Ok(f) => f,
+        Err(_) => return "\0DELETED".to_string(),
+    };
+    let mut hasher = Sha256::new();
+    let mut reader = BufReader::new(f);
+    let mut buf = [0u8; 64 * 1024];
+    loop {
+        match reader.read(&mut buf) {
+            Ok(0) => break,
+            Ok(n) => hasher.update(&buf[..n]),
+            Err(_) => return "\0UNREADABLE".to_string(),
+        }
+    }
+    hex(&hasher.finalize())
+}
+
+/// Lowercase hex encoding (avoids pulling in the `hex` crate).
+fn hex(bytes: &[u8]) -> String {
+    use std::fmt::Write as _;
+    let mut s = String::with_capacity(bytes.len() * 2);
+    for b in bytes {
+        let _ = write!(s, "{b:02x}");
+    }
+    s
+}
+
+fn serialize(cf: &CacheFile) -> io::Result<Vec<u8>> {
+    serde_json::to_vec_pretty(cf).map_err(io::Error::other)
+}
+
+/// Atomic write with user-only permissions: write a temp file in the same dir
+/// (created `0600` *before* any content lands, so the plan is never briefly
+/// world-readable), then rename over the target.
+fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
+    let dir = path
+        .parent()
+        .ok_or_else(|| io::Error::other("cache path has no parent"))?;
+    fs::create_dir_all(dir)?;
+    let tmp = dir.join(format!(".plan-{}.tmp", std::process::id()));
+    {
+        let mut f = open_private(&tmp)?;
+        f.write_all(data)?;
+        f.sync_all()?;
+    }
+    fs::rename(&tmp, path)
+}
+
+#[cfg(unix)]
+fn open_private(path: &Path) -> io::Result<fs::File> {
+    use std::os::unix::fs::OpenOptionsExt;
+    fs::OpenOptions::new()
+        .write(true)
+        .create(true)
+        .truncate(true)
+        .mode(0o600)
+        .open(path)
+}
+
+#[cfg(not(unix))]
+fn open_private(path: &Path) -> io::Result<fs::File> {
+    fs::OpenOptions::new()
+        .write(true)
+        .create(true)
+        .truncate(true)
+        .open(path)
+}
+
+fn to_io(e: GcmError) -> io::Error {
+    io::Error::other(e.to_string())
+}
+
+fn no_cache_dir() -> io::Error {
+    io::Error::other("no OS cache directory available")
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+    use crate::plan::{Group, Plan};
+
+    fn group(files: &[&str], msg: Option<&str>) -> Group {
+        Group {
+            files: files.iter().map(|s| s.to_string()).collect(),
+            summary: "s".to_string(),
+            commit_message: msg.map(|m| m.to_string()),
+        }
+    }
+
+    fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
+        pairs
+            .iter()
+            .map(|(p, h)| (p.to_string(), h.to_string()))
+            .collect()
+    }
+
+    #[test]
+    fn hex_encodes_lowercase_padded() {
+        assert_eq!(hex(&[0x00, 0x0f, 0xff, 0xa0]), "000fffa0");
+    }
+
+    #[test]
+    fn repo_key_is_stable_and_path_specific() {
+        let a = repo_key(Path::new("/home/u/repo"));
+        let b = repo_key(Path::new("/home/u/repo"));
+        let c = repo_key(Path::new("/home/u/other"));
+        assert_eq!(a, b, "same path -> same key");
+        assert_ne!(a, c, "different path -> different key");
+        assert_eq!(a.len(), 64, "full sha256 hex");
+    }
+
+    #[test]
+    fn cache_file_name_is_plan_prefixed_json() {
+        let name = cache_file_name(Path::new("/home/u/repo"));
+        assert!(name.starts_with("plan-"), "name: {name}");
+        assert!(name.ends_with(".json"), "name: {name}");
+        assert!(!name.contains('/'), "single path component, not /tmp/...");
+    }
+
+    #[test]
+    fn fingerprint_is_stable_for_same_inputs() {
+        let e = entries(&[("a.rs", "h1"), ("b.rs", "h2")]);
+        assert_eq!(
+            digest_fingerprint("groq:m", &e),
+            digest_fingerprint("groq:m", &e)
+        );
+    }
+
+    #[test]
+    fn fingerprint_flips_on_content_change() {
+        let before = entries(&[("a.rs", "h1")]);
+        let after = entries(&[("a.rs", "h2")]); // same name, different content hash
+        assert_ne!(
+            digest_fingerprint("m", &before),
+            digest_fingerprint("m", &after),
+            "a content change (not a name change) must invalidate"
+        );
+    }
+
+    #[test]
+    fn fingerprint_flips_on_file_set_change() {
+        let one = entries(&[("a.rs", "h1")]);
+        let two = entries(&[("a.rs", "h1"), ("b.rs", "h2")]);
+        assert_ne!(digest_fingerprint("m", &one), digest_fingerprint("m", &two));
+    }
+
+    #[test]
+    fn fingerprint_flips_on_model_change() {
+        let e = entries(&[("a.rs", "h1")]);
+        assert_ne!(
+            digest_fingerprint("groq:model-a", &e),
+            digest_fingerprint("groq:model-b", &e),
+            "switching provider/model must invalidate"
+        );
+    }
+
+    #[test]
+    fn deletion_marker_differs_from_a_real_hash() {
+        // A pending deletion must not collide with any content hash.
+        let present = entries(&[("a.rs", "deadbeef")]);
+        let deleted = entries(&[("a.rs", "\0DELETED")]);
+        assert_ne!(
+            digest_fingerprint("m", &present),
+            digest_fingerprint("m", &deleted)
+        );
+    }
+
+    #[test]
+    fn remaining_groups_drops_the_first() {
+        let plan = Plan {
+            groups: vec![
+                group(&["a.rs"], Some("feat: a")),
+                group(&["b.rs"], None),
+                group(&["c.rs"], None),
+            ],
+        };
+        let rem = remaining_groups(&plan);
+        assert_eq!(rem.groups.len(), 2);
+        assert_eq!(rem.groups[0].files, vec!["b.rs"]);
+        assert_eq!(rem.groups[1].files, vec!["c.rs"]);
+    }
+
+    #[test]
+    fn remaining_groups_of_single_group_is_empty() {
+        let plan = Plan {
+            groups: vec![group(&["a.rs"], Some("feat: a"))],
+        };
+        assert!(
+            remaining_groups(&plan).groups.is_empty(),
+            "delete on advance"
+        );
+    }
+
+    #[test]
+    fn read_cache_file_round_trips_a_valid_file() {
+        let cf = CacheFile {
+            version: CACHE_FORMAT_VERSION,
+            fingerprint: "fp".to_string(),
+            plan: Plan {
+                groups: vec![group(&["a.rs"], Some("feat: a")), group(&["b.rs"], None)],
+            },
+        };
+        let bytes = serialize(&cf).unwrap();
+        let back = read_cache_file(&bytes).expect("valid");
+        assert_eq!(back.fingerprint, "fp");
+        assert_eq!(back.plan.groups.len(), 2);
+        assert_eq!(back.plan.groups[1].commit_message, None);
+    }
+
+    #[test]
+    fn read_cache_file_rejects_wrong_format_version() {
+        let json = br#"{"version":0,"fingerprint":"fp","plan":{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"feat: a"}]}}"#;
+        assert!(
+            read_cache_file(json).is_none(),
+            "old format version -> miss"
+        );
+    }
+
+    #[test]
+    fn read_cache_file_rejects_corrupt_json() {
+        assert!(read_cache_file(b"not json at all").is_none());
+        assert!(read_cache_file(b"").is_none());
+    }
+
+    #[test]
+    fn read_cache_file_rejects_empty_plan() {
+        let json = br#"{"version":1,"fingerprint":"fp","plan":{"groups":[]}}"#;
+        assert!(read_cache_file(json).is_none());
+        let empty_g0 = br#"{"version":1,"fingerprint":"fp","plan":{"groups":[{"files":[],"summary":"s","commit_message":"m"}]}}"#;
+        assert!(read_cache_file(empty_g0).is_none());
+    }
+}
diff --git a/src/cli.rs b/src/cli.rs
index 3029b6e..6388dfd 100644
--- a/src/cli.rs
+++ b/src/cli.rs
@@ -1,37 +1,41 @@
 use clap::Parser;
 
 /// Build-stamped version: crate version plus the git short SHA from build.rs (AC-1).
 pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GCM_GIT_SHA"));
 
 const EGRESS_DISCLOSURE: &str = "\
 gcm groups your working-tree changes into logical commits and commits the first group;\n\
 run it again to commit the next group. Grouping operates on whole files over the entire\n\
 working tree, so it overrides any manual hunk-level (git add -p) staging: group 1's files\n\
 are staged in full, later groups are left unstaged (their changes are never lost).\n\
 \n\
 PRIVACY: gcm sends your working-tree diff and the content of untracked, non-gitignored\n\
 files to the configured LLM provider (Groq) to generate the plan and commit messages.\n\
 Gitignored files (e.g. .env) are never sent. See the README for each provider's data policy.";
 
 #[derive(Parser, Debug)]
 #[command(
     name = "gcm",
     version = VERSION,
     about = "Generate one signed conventional-commit from your working-tree changes via Groq.",
     after_help = EGRESS_DISCLOSURE,
     after_long_help = EGRESS_DISCLOSURE
 )]
 pub struct Cli {
     /// Preview the grouping plan (or the single-commit message with --all) and
     /// exit without staging or committing.
     #[arg(long)]
     pub dry_run: bool,
 
     /// Skip grouping and commit all changes as a single commit.
     #[arg(long)]
     pub all: bool,
 
+    /// Discard any cached grouping plan and re-analyze from scratch.
+    #[arg(long)]
+    pub reset: bool,
+
     /// Auto-confirm the commit without prompting (for non-interactive / agent / CI use).
     #[arg(long, visible_alias = "no-input")]
     pub yes: bool,
 }
diff --git a/src/diff.rs b/src/diff.rs
index c52f18f..05eb42d 100644
--- a/src/diff.rs
+++ b/src/diff.rs
@@ -1,190 +1,221 @@
+use std::collections::HashSet;
 use std::io::Read;
 use std::path::Path;
 
 use crate::error::GcmError;
 use crate::git::{ChangedFile, Repo};
 
 /// JSON-encode the changed-file paths as an array of strings so a path
 /// containing a newline (or any character) stays a single discrete element in
 /// the grouping prompt - newline-joining would split such a path into multiple
 /// lines and the model would group phantom paths (CLO-487 path-agreement).
 fn file_list_json(changed: &[ChangedFile]) -> String {
     let paths: Vec<&str> = changed.iter().map(|c| c.path.as_str()).collect();
     serde_json::to_string(&paths).unwrap_or_else(|_| "[]".to_string())
 }
 
 /// JSON-encode the porcelain status as an array of `"XY path"` strings (also
 /// newline-safe, same rationale as [`file_list_json`]).
 fn status_json(changed: &[ChangedFile]) -> String {
     let rows: Vec<String> = changed
         .iter()
         .map(|c| format!("{}{} {}", c.x as char, c.y as char, c.path))
         .collect();
     serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
 }
 
 /// Untracked-expansion caps (FR-57): bound both file count and total bytes so an
 /// un-ignored directory of thousands of files cannot freeze the CLI.
 const MAX_UNTRACKED_FILES: usize = 50;
 const MAX_UNTRACKED_BYTES: usize = 256 * 1024;
 /// Per-file read cap for an individual untracked file (mirrors bash `head -c 8192`).
 const PER_FILE_BYTES: usize = 8192;
 /// Per-file cap for a tracked diff section in the grouping prompt: each file's
 /// section is truncated independently with a `[diff omitted: N bytes]`
 /// placeholder rather than tail-chopping the whole body (CLO-487 FR-15).
 const PER_FILE_DIFF_BYTES: usize = 8192;
 /// Coarse final safeguard on the whole assembled body.
 const MAX_TOTAL_BYTES: usize = 350_000;
 
 /// The diff context handed to the provider.
 pub struct GatheredDiff {
     pub stat: String,
     pub body: String,
 }
 
 /// The richer context handed to the provider for grouping (CLO-487): the file
 /// list and porcelain status (both JSON arrays, so newline-containing paths stay
 /// discrete), the diff `--stat`, and the per-file-truncated full diff. Distinct
 /// from [`GatheredDiff`] to keep the tracer's single-message concerns separate.
 pub struct GroupingContext {
     /// JSON array of the exact changed paths (the model groups by these).
     pub file_list: String,
     /// JSON array of `"XY path"` porcelain status rows.
     pub status: String,
     pub stat: String,
     pub body: String,
 }
 
 /// Build the prompt diff: tracked changes (binary-elided) plus untracked,
 /// non-gitignored file content, bounded by the FR-57 caps. Reads only the
 /// working tree; nothing is staged (FR-47).
 pub fn gather(repo: &Repo) -> Result<GatheredDiff, GcmError> {
     let stat = repo.diff_stat()?;
     let tracked = repo.diff_full()?;
     let mut body = elide_binary_diff(&tracked);
-    append_untracked(repo, &mut body)?;
+    append_untracked(repo, &mut body, None)?;
+    cap_total(&mut body);
+    Ok(GatheredDiff { stat, body })
+}
+
+/// Build the single-message diff for **one commit group** (CLO-491, FR-45): the
+/// tracked diff and stat scoped to the group's paths, plus the group's own
+/// untracked files (filtered, so other groups' untracked content never leaks
+/// into this message). Used to regenerate a message-only call for an advanced
+/// group on a cache hit. Unborn-safe: with no `HEAD` the tracked diff is empty
+/// and all content arrives through the filtered untracked path.
+pub fn gather_for_files(repo: &Repo, files: &[&ChangedFile]) -> Result<GatheredDiff, GcmError> {
+    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
+    let stat = repo.diff_stat_for(&paths)?;
+    let tracked = repo.diff_full_for(&paths)?;
+    let mut body = elide_binary_diff(&tracked);
+    let allow: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
+    append_untracked(repo, &mut body, Some(&allow))?;
     cap_total(&mut body);
     Ok(GatheredDiff { stat, body })
 }
 
 /// Build the grouping context (CLO-487): the file list and porcelain status are
 /// derived from the already-gathered `changed` set (so they stay byte-identical
 /// to the paths used for validation and staging), the diff `--stat` is the
 /// prompt header, and the body is the tracked diff truncated **per file** with
 /// `[diff omitted: N bytes]` placeholders, plus untracked content (FR-57 caps),
 /// under the `MAX_TOTAL_BYTES` final safeguard.
 pub fn gather_for_grouping(
     repo: &Repo,
     changed: &[ChangedFile],
 ) -> Result<GroupingContext, GcmError> {
     let file_list = file_list_json(changed);
     let status = status_json(changed);
 
     let stat = repo.diff_stat()?;
     let tracked = repo.diff_full()?;
     let mut body = truncate_per_file(&elide_binary_diff(&tracked), PER_FILE_DIFF_BYTES);
-    append_untracked(repo, &mut body)?;
+    append_untracked(repo, &mut body, None)?;
     cap_total(&mut body);
 
     Ok(GroupingContext {
         file_list,
         status,
         stat,
         body,
     })
 }
 
 /// Append untracked, non-gitignored file content to `body`, bounded by the
 /// FR-57 file-count and byte caps. Shared by [`gather`] and
-/// [`gather_for_grouping`] so the two prompts cannot diverge.
-fn append_untracked(repo: &Repo, body: &mut String) -> Result<(), GcmError> {
+/// [`gather_for_grouping`] (which pass `None` = every untracked file) and
+/// [`gather_for_files`] (which passes `Some(allow)` to restrict to one group's
+/// paths, so a single group's message diff is not polluted by other groups'
+/// untracked files - CLO-491).
+fn append_untracked(
+    repo: &Repo,
+    body: &mut String,
+    allow: Option<&HashSet<String>>,
+) -> Result<(), GcmError> {
     let mut untracked = repo.untracked_files()?;
     untracked.sort();
 
-    // Every untracked path counts toward the file-count cap - binary and
-    // unreadable files included - so a directory of thousands of files (of any
-    // kind) cannot force thousands of reads. Once either cap is reached, every
-    // remaining file is listed by name only, with no read at all (FR-57).
+    // Every (allow-listed) untracked path counts toward the file-count cap -
+    // binary and unreadable files included - so a directory of thousands of
+    // files (of any kind) cannot force thousands of reads. Once either cap is
+    // reached, every remaining file is listed by name only, with no read at all
+    // (FR-57).
     let mut files_done = 0usize;
     let mut bytes_used = 0usize;
     for path in &untracked {
+        // Filter to the allow-list (if any) before the caps, so excluded paths
+        // neither consume the budget nor reach the prompt.
+        if allow.is_some_and(|a| !a.contains(path)) {
+            continue;
+        }
         if files_done >= MAX_UNTRACKED_FILES || bytes_used >= MAX_UNTRACKED_BYTES {
             body.push_str(&format!(
                 "\n--- /dev/null\n+++ b/{path}\n[content omitted: untracked cap reached]\n"
             ));
             continue;
         }
         let full = repo.root().join(path);
         // Only read regular files. `symlink_metadata` does not follow symlinks,
         // so we never read a symlink's target (which could leak content from
         // outside the repo) and never block on a FIFO/device/socket.
         let is_regular = std::fs::symlink_metadata(&full)
             .map(|m| m.file_type().is_file())
             .unwrap_or(false);
         if !is_regular {
             body.push_str(&format!(
                 "\n--- /dev/null\n+++ b/{path}\n[omitted: not a regular file]\n"
             ));
             files_done += 1;
             continue;
         }
         // Read at most a per-file slice bounded by the remaining byte budget, so
         // a single huge file is never loaded into memory in full.
         let budget = (MAX_UNTRACKED_BYTES - bytes_used).min(PER_FILE_BYTES);
         match read_capped(&full, budget) {
             Ok((content, more)) if looks_binary(&content) => {
                 body.push_str(&format!("\n--- /dev/null\n+++ b/{path}\n+[binary file]\n"));
                 let _ = more;
             }
             Ok((content, more)) => {
                 let text = String::from_utf8_lossy(&content);
                 body.push_str(&format!("\n--- /dev/null\n+++ b/{path}\n"));
                 for line in text.lines() {
                     body.push('+');
                     body.push_str(line);
                     body.push('\n');
                 }
                 if more {
                     body.push_str("+[truncated]\n");
                 }
                 bytes_used += content.len();
             }
             Err(_) => {
                 // Unreadable (perm, race, symlink loop) - note by name, never block.
                 body.push_str(&format!(
                     "\n--- /dev/null\n+++ b/{path}\n[omitted: unreadable]\n"
                 ));
             }
         }
         files_done += 1;
     }
     Ok(())
 }
 
 /// Coarse final safeguard on the whole assembled body (FR-57), truncating on a
 /// char boundary so a multibyte char split at the cap does not panic.
 fn cap_total(body: &mut String) {
     if body.len() > MAX_TOTAL_BYTES {
         let mut end = MAX_TOTAL_BYTES;
         while end > 0 && !body.is_char_boundary(end) {
             end -= 1;
         }
         body.truncate(end);
         body.push_str("\n... (diff truncated)\n");
     }
 }
 
 /// Truncate a tracked diff **per file**: split on `diff --git ` boundaries and,
 /// for any section longer than `cap`, keep the file's header and replace its
 /// hunk body with `[diff omitted: N bytes]` (N = omitted bytes). This keeps
 /// every changed file present in the prompt instead of tail-chopping the whole
 /// body and severing the last file mid-hunk (CLO-487 FR-15).
 fn truncate_per_file(diff: &str, cap: usize) -> String {
     let mut out = String::new();
     let mut section = String::new();
     for line in diff.split_inclusive('\n') {
         if line.starts_with("diff --git ") && !section.is_empty() {
             push_capped_section(&section, cap, &mut out);
             section.clear();
         }
         section.push_str(line);
diff --git a/src/error.rs b/src/error.rs
index 1fd5226..80ed59a 100644
--- a/src/error.rs
+++ b/src/error.rs
@@ -1,58 +1,102 @@
 use std::fmt;
 
 use crate::groq::GroqError;
 
 /// Top-level runtime error. CLI usage errors are handled by clap (exit 2);
 /// every variant here maps to exit code 1. User abort is not an error and is
 /// represented as a successful `Outcome`, not a `GcmError`.
 #[derive(Debug)]
 pub enum GcmError {
     NotARepo,
     Git(String),
     Groq(GroqError),
     /// Non-TTY context without `--yes`/`--no-input`: cannot prompt (ADR-001 #10).
     NonInteractive,
     Editor(String),
     EmptyMessage,
     /// The repository has unresolved merge conflicts (unmerged index entries).
     /// gcm aborts rather than risk committing conflict markers (CLO-487).
     UnmergedConflicts,
+    /// `git commit` itself failed after the group was staged (e.g. a rejecting
+    /// pre-commit hook, a signing failure). The group is left **staged** and the
+    /// plan cache is **not** advanced so the user can fix and retry (CLO-491,
+    /// FR-58). Distinct from [`GcmError::Git`] (pre-commit-step failures, which
+    /// restore the index).
+    CommitFailed(String),
 }
 
 impl GcmError {
     /// Process exit code for this error. All runtime errors are 1; usage (exit 2)
     /// is produced by clap before we get here.
     pub fn exit_code(&self) -> i32 {
         1
     }
+
+    /// Whether this error means the staged group should be **left in place**.
+    /// Only a commit-step failure ([`GcmError::CommitFailed`]) leaves the group
+    /// staged (FR-58); every other error restores the pre-run index (FR-47).
+    pub fn leaves_staged(&self) -> bool {
+        matches!(self, GcmError::CommitFailed(_))
+    }
 }
 
 impl fmt::Display for GcmError {
     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
         match self {
             GcmError::NotARepo => {
                 write!(f, "not a git repository (run gcm inside a git work tree)")
             }
             GcmError::Git(msg) => write!(f, "{msg}"),
             GcmError::Groq(e) => write!(f, "{e}"),
             GcmError::NonInteractive => write!(
                 f,
                 "no terminal available to confirm the commit. Re-run with --yes (or --no-input) \
                  to auto-confirm, or --dry-run to preview without committing."
             ),
             GcmError::Editor(msg) => write!(f, "editor failed: {msg}"),
             GcmError::EmptyMessage => write!(f, "commit message is empty; nothing committed"),
             GcmError::UnmergedConflicts => write!(
                 f,
                 "repository has unresolved merge conflicts; resolve them and stage your \
                  resolution before running gcm"
             ),
+            GcmError::CommitFailed(msg) => write!(
+                f,
+                "{msg}\nThe group is left staged and the plan was not advanced; \
+                 fix the issue and re-run gcm to retry this group."
+            ),
         }
     }
 }
 
 impl From<GroqError> for GcmError {
     fn from(e: GroqError) -> Self {
         GcmError::Groq(e)
     }
 }
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    #[test]
+    fn only_commit_failed_leaves_the_group_staged() {
+        // FR-58: a commit-step failure leaves the group staged; every other
+        // error restores the pre-run index (FR-47).
+        assert!(GcmError::CommitFailed("hook rejected".to_string()).leaves_staged());
+        assert!(!GcmError::Git("git add failed".to_string()).leaves_staged());
+        assert!(!GcmError::UnmergedConflicts.leaves_staged());
+        assert!(!GcmError::NotARepo.leaves_staged());
+    }
+
+    #[test]
+    fn commit_failed_surfaces_the_underlying_error() {
+        let msg =
+            GcmError::CommitFailed("git commit failed (see output above)".to_string()).to_string();
+        assert!(msg.contains("git commit failed"));
+        assert!(
+            msg.contains("left staged"),
+            "tells the user the group is kept"
+        );
+    }
+}
diff --git a/src/git.rs b/src/git.rs
index 6c04e57..62d0dcf 100644
--- a/src/git.rs
+++ b/src/git.rs
@@ -42,210 +42,269 @@ impl Repo {
     /// A `git` Command rooted at the repo with quotePath disabled.
     fn git(&self, args: &[&str]) -> Command {
         let mut c = Command::new("git");
         c.current_dir(&self.root);
         c.args(["-c", "core.quotePath=false"]);
         c.args(args);
         c
     }
 
     /// Run a git command, capturing stdout as a (lossy) UTF-8 string.
     fn capture(&self, args: &[&str]) -> Result<String, GcmError> {
         let out = self
             .git(args)
             .output()
             .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", args.join(" "))))?;
         if !out.status.success() {
             return Err(GcmError::Git(format!(
                 "git {} failed: {}",
                 args.join(" "),
                 String::from_utf8_lossy(&out.stderr).trim()
             )));
         }
         Ok(String::from_utf8_lossy(&out.stdout).into_owned())
     }
 
     /// Whether HEAD resolves (false on an unborn branch / fresh repo).
     pub fn has_head(&self) -> bool {
         self.git(&["rev-parse", "--verify", "--quiet", "HEAD"])
             .output()
             .map(|o| o.status.success())
             .unwrap_or(false)
     }
 
     /// True if there are any uncommitted changes: unstaged, staged, or untracked
     /// (gitignore-respecting). Drives the "no changes -> exit 0" path (FR-9).
     pub fn has_changes(&self) -> Result<bool, GcmError> {
         let unstaged = !self.quiet_diff(&["diff", "--quiet"])?;
         let staged = !self.quiet_diff(&["diff", "--cached", "--quiet"])?;
         let untracked = !self.untracked_files()?.is_empty();
         Ok(unstaged || staged || untracked)
     }
 
     /// Run a `--quiet` diff; returns true when there is NO difference (exit 0).
     fn quiet_diff(&self, args: &[&str]) -> Result<bool, GcmError> {
         let status = self
             .git(args)
             .status()
             .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", args.join(" "))))?;
         Ok(status.success())
     }
 
     /// Diff stat for the prompt header. With HEAD, `git diff HEAD` covers all
     /// tracked changes. On an unborn branch (no HEAD) the empty-tree object may
     /// not exist in a fresh repo (so `git diff <empty-tree>` errors), thus we
     /// combine unstaged (working vs index) and staged (index vs empty) diffs -
     /// together they capture every tracked change, incl. a staged-then-modified
     /// file - and gather untracked files separately (AC-14).
     pub fn diff_stat(&self) -> Result<String, GcmError> {
         if self.has_head() {
             self.capture(&["diff", "--stat", "HEAD"])
         } else {
             let unstaged = self.capture(&["diff", "--stat"])?;
             let staged = self.capture(&["diff", "--cached", "--stat"])?;
             Ok(format!("{unstaged}{staged}"))
         }
     }
 
     /// Full diff (no color) for the prompt body. HEAD when present; otherwise
     /// unstaged + staged on an unborn branch. See [`Self::diff_stat`] for the
     /// unborn-branch rationale.
     pub fn diff_full(&self) -> Result<String, GcmError> {
         if self.has_head() {
             self.capture(&["diff", "--no-color", "HEAD"])
         } else {
             let unstaged = self.capture(&["diff", "--no-color"])?;
             let staged = self.capture(&["diff", "--no-color", "--cached"])?;
             Ok(format!("{unstaged}{staged}"))
         }
     }
 
+    /// Diff `--stat` scoped to specific paths (CLO-491 per-group message header).
+    /// Same HEAD/unborn handling as [`Self::diff_stat`]. Empty `paths` returns an
+    /// empty string rather than an unscoped whole-tree diff.
+    pub fn diff_stat_for(&self, paths: &[&str]) -> Result<String, GcmError> {
+        if paths.is_empty() {
+            return Ok(String::new());
+        }
+        if self.has_head() {
+            self.capture_scoped(&["diff", "--stat", "HEAD"], paths)
+        } else {
+            let unstaged = self.capture_scoped(&["diff", "--stat"], paths)?;
+            let staged = self.capture_scoped(&["diff", "--stat", "--cached"], paths)?;
+            Ok(format!("{unstaged}{staged}"))
+        }
+    }
+
+    /// Full diff (no color) scoped to specific paths (CLO-491 per-group message
+    /// body). Same HEAD/unborn handling as [`Self::diff_full`]. Empty `paths`
+    /// returns an empty string.
+    pub fn diff_full_for(&self, paths: &[&str]) -> Result<String, GcmError> {
+        if paths.is_empty() {
+            return Ok(String::new());
+        }
+        if self.has_head() {
+            self.capture_scoped(&["diff", "--no-color", "HEAD"], paths)
+        } else {
+            let unstaged = self.capture_scoped(&["diff", "--no-color"], paths)?;
+            let staged = self.capture_scoped(&["diff", "--no-color", "--cached"], paths)?;
+            Ok(format!("{unstaged}{staged}"))
+        }
+    }
+
+    /// Like [`Self::capture`] but appends `-- <paths>` with
+    /// `GIT_LITERAL_PATHSPECS=1`, so a filename containing a glob metacharacter
+    /// (`*`, `?`) cannot pull in siblings (the CLO-487 review-2 #3 hazard).
+    fn capture_scoped(&self, base: &[&str], paths: &[&str]) -> Result<String, GcmError> {
+        let mut cmd = self.git(base);
+        cmd.env("GIT_LITERAL_PATHSPECS", "1");
+        cmd.arg("--");
+        cmd.args(paths);
+        let out = cmd
+            .output()
+            .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", base.join(" "))))?;
+        if !out.status.success() {
+            return Err(GcmError::Git(format!(
+                "git {} failed: {}",
+                base.join(" "),
+                String::from_utf8_lossy(&out.stderr).trim()
+            )));
+        }
+        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
+    }
+
     /// Untracked files honoring gitignore (`--exclude-standard`), NUL-split so
     /// unicode/space/newline paths survive (FR-31, FR-48).
     pub fn untracked_files(&self) -> Result<Vec<String>, GcmError> {
         let out = self
             .git(&["ls-files", "--others", "--exclude-standard", "-z"])
             .output()
             .map_err(|e| GcmError::Git(format!("failed to run git ls-files: {e}")))?;
         if !out.status.success() {
             return Err(GcmError::Git(format!(
                 "git ls-files failed: {}",
                 String::from_utf8_lossy(&out.stderr).trim()
             )));
         }
         Ok(out
             .stdout
             .split(|&b| b == 0)
             .filter(|s| !s.is_empty())
             .map(|s| String::from_utf8_lossy(s).into_owned())
             .collect())
     }
 
     /// Capture the current index as a tree object (FR-47 transaction start).
     pub fn snapshot_index(&self) -> Result<String, GcmError> {
         Ok(self.capture(&["write-tree"])?.trim().to_string())
     }
 
     /// Restore the index to a previously-snapshotted tree. The working tree is
     /// untouched; this only rewinds staging (FR-47 restore on abort/failure).
     pub fn restore_index(&self, tree: &str) -> Result<(), GcmError> {
         self.capture(&["read-tree", tree]).map(|_| ())
     }
 
     /// Stage every change (the tracer commits all changes as one commit, FR-6).
     pub fn stage_all(&self) -> Result<(), GcmError> {
         self.capture(&["add", "-A"]).map(|_| ())
     }
 
     /// Create a signed commit (FR-4). Stdio is inherited so GPG/SSH passphrase
     /// (pinentry) prompts work on the user's terminal.
+    ///
+    /// A non-zero `git commit` (a rejecting pre-commit hook, a signing failure)
+    /// returns [`GcmError::CommitFailed`], not [`GcmError::Git`]: the caller
+    /// leaves the staged group in place and does not advance the plan cache
+    /// (CLO-491, FR-58). A failure to even spawn `git` is a `Git` error (no
+    /// commit was attempted, so the staged group should be rolled back).
     pub fn commit_signed(&self, message: &str) -> Result<(), GcmError> {
         let status = self
             .git(&["commit", "-S", "-m", message])
             .stdin(Stdio::inherit())
             .stdout(Stdio::inherit())
             .stderr(Stdio::inherit())
             .status()
             .map_err(|e| GcmError::Git(format!("failed to run git commit: {e}")))?;
         if !status.success() {
-            return Err(GcmError::Git(
-                "git commit failed (see output above); index restored".to_string(),
+            return Err(GcmError::CommitFailed(
+                "git commit failed (see output above)".to_string(),
             ));
         }
         Ok(())
     }
 
     /// The full changed-file set for grouping, from
     /// `git status --porcelain=v1 -uall -z`. `-uall` expands untracked
     /// directories to individual files so these paths match the per-file diff
     /// paths (CLO-487 review-2 #1). NUL-delimited; renames carry their orig path.
     pub fn changed_files(&self) -> Result<Vec<ChangedFile>, GcmError> {
         let out = self
             .git(&["status", "--porcelain=v1", "-uall", "-z"])
             .output()
             .map_err(|e| GcmError::Git(format!("failed to run git status: {e}")))?;
         if !out.status.success() {
             return Err(GcmError::Git(format!(
                 "git status failed: {}",
                 String::from_utf8_lossy(&out.stderr).trim()
             )));
         }
         Ok(parse_status_z(&out.stdout))
     }
 
     /// True if a merge is in progress (`.git/MERGE_HEAD` exists). Combined with
     /// [`ChangedFile::is_unmerged`] this distinguishes a clean merge (commit it)
     /// from a conflicted one (abort) - CLO-487 review-2 #2.
     pub fn is_merging(&self) -> bool {
         self.git(&["rev-parse", "--verify", "--quiet", "MERGE_HEAD"])
             .output()
             .map(|o| o.status.success())
             .unwrap_or(false)
     }
 
     /// Reset the index to the committed state so a subsequent path-scoped
     /// `add` produces a commit of exactly those paths: `read-tree HEAD` when
     /// HEAD resolves, `read-tree --empty` on an unborn branch (no HEAD - plain
     /// `read-tree HEAD` would fail). Clearing to HEAD (not emptying) keeps
     /// other tracked files at their HEAD version so they are not recorded as
     /// deletions (CLO-487 review-1 #2).
     pub fn clear_staged(&self) -> Result<(), GcmError> {
         if self.has_head() {
             self.capture(&["read-tree", "HEAD"]).map(|_| ())
         } else {
             self.capture(&["read-tree", "--empty"]).map(|_| ())
         }
     }
 
     /// Stage exactly the given files (a commit group). Paths are fed
     /// NUL-separated on stdin via `--pathspec-from-file=- --pathspec-file-nul`
     /// (no `ARG_MAX` limit, no arg quoting) and `GIT_LITERAL_PATHSPECS=1`
     /// disables git's internal pathspec globbing so a filename containing `*`
     /// or `?` cannot pull in siblings (CLO-487 review-2 #3 + #4). Rename/copy
     /// entries contribute both their new and original path so the commit
     /// completes the rename (review-1 #1).
     pub fn stage_group(&self, files: &[&ChangedFile]) -> Result<(), GcmError> {
         let mut stdin_bytes: Vec<u8> = Vec::new();
         for cf in files {
             for p in cf.stage_paths() {
                 stdin_bytes.extend_from_slice(p.as_bytes());
                 stdin_bytes.push(0);
             }
         }
         let mut child = self
             .git(&["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"])
             .env("GIT_LITERAL_PATHSPECS", "1")
             .stdin(Stdio::piped())
             .stdout(Stdio::piped())
             .stderr(Stdio::piped())
             .spawn()
             .map_err(|e| GcmError::Git(format!("failed to run git add: {e}")))?;
         child
             .stdin
             .take()
             .expect("piped stdin")
             .write_all(&stdin_bytes)
             .map_err(|e| GcmError::Git(format!("failed to write pathspecs to git add: {e}")))?;
         let out = child
             .wait_with_output()
             .map_err(|e| GcmError::Git(format!("failed to run git add: {e}")))?;
         if !out.status.success() {
diff --git a/src/groq.rs b/src/groq.rs
index 7a33bef..c82cb3b 100644
--- a/src/groq.rs
+++ b/src/groq.rs
@@ -1,170 +1,177 @@
 use std::fmt;
 use std::time::Duration;
 
 use serde::Deserialize;
 use serde_json::{json, Value};
 
 use crate::diff::{GatheredDiff, GroupingContext};
 use crate::plan::Plan;
 
 const DEFAULT_MODEL: &str = "openai/gpt-oss-120b";
 const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
 const TIMEOUT_SECS: u64 = 30;
 
 const SYSTEM_PROMPT: &str = "\
 Analyze this git diff and generate a concise, conventional commit message.
 Use format: <type>(<scope>): <description>
 Types: feat, fix, docs, style, refactor, test, chore
 Keep the first line under 72 characters.
 Add a blank line and bullet points for details if there are multiple significant changes.
 Do NOT include any explanation - output ONLY the commit message.";
 
 /// System prompt for the grouping plan (CLO-487; adapted from the bash tool,
 /// `docs/tmp/git-commit-ai.sh:305-322`). The `response_format` json_schema
 /// enforces the output shape, so the prompt carries only the grouping rules.
 const GROUPING_SYSTEM_PROMPT: &str = "\
 Analyze these git changes. Group related files into logical commits by semantic relevance.
 
 Rules:
 - Every file from the file list must appear in exactly one group.
 - Prefer fewer groups (1-3) unless changes are truly unrelated.
 - commit_message: a full conventional-commit message for groups[0] ONLY; null for every other group.
 - Conventional format <type>(<scope>): <description>, first line under 72 chars; add a blank line
   and bullet points for details when there are multiple significant changes.
 - For renamed files, use the NEW path in your file list.
 - summary: a one-line description of each group.";
 
 /// Errors from the Groq message call. A light taxonomy for the tracer; the full
 /// typed-error/retry surface (FR-21/22) lands in CLO-488.
 #[derive(Debug)]
 pub enum GroqError {
     MissingKey,
     Http(u16),
     Timeout,
     Transport(String),
     EmptyResponse,
     Deserialize(String),
 }
 
 impl fmt::Display for GroqError {
     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
         match self {
             GroqError::MissingKey => write!(
                 f,
                 "GROQ_API_KEY is not set. Export it (e.g. `export GROQ_API_KEY=...`) and retry."
             ),
             GroqError::Http(code) => write!(f, "Groq API returned HTTP {code}"),
             GroqError::Timeout => write!(f, "Groq API request timed out after {TIMEOUT_SECS}s"),
             GroqError::Transport(msg) => write!(f, "could not reach the Groq API: {msg}"),
             GroqError::EmptyResponse => write!(f, "Groq returned an empty commit message"),
             GroqError::Deserialize(msg) => write!(f, "could not parse the Groq response: {msg}"),
         }
     }
 }
 
 #[derive(Deserialize)]
 struct ChatResponse {
     choices: Vec<Choice>,
 }
 
 #[derive(Deserialize)]
 struct Choice {
     message: Message,
 }
 
 #[derive(Deserialize)]
 struct Message {
     content: Option<String>,
 }
 
+/// The configured model id (`GCM_GROQ_MODEL` or the default), resolved
+/// **without** requiring `GROQ_API_KEY`. Used by the plan cache to fold the
+/// model into the freshness fingerprint (CLO-491, FR-27) even when no key is set.
+pub fn resolved_model() -> String {
+    std::env::var("GCM_GROQ_MODEL")
+        .ok()
+        .filter(|m| !m.trim().is_empty())
+        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
+}
+
 /// Resolve `(api_key, model, base_url)` from the environment - shared by the
 /// message (tracer) and plan (grouping) calls.
 fn resolve_config() -> Result<(String, String, String), GroqError> {
     let key = std::env::var("GROQ_API_KEY")
         .ok()
         .filter(|k| !k.trim().is_empty())
         .ok_or(GroqError::MissingKey)?;
-    let model = std::env::var("GCM_GROQ_MODEL")
-        .ok()
-        .filter(|m| !m.trim().is_empty())
-        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
+    let model = resolved_model();
     let base_url = std::env::var("GCM_GROQ_BASE_URL")
         .ok()
         .filter(|u| !u.trim().is_empty())
         .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
     Ok((key, model, base_url))
 }
 
 /// POST a chat-completions payload and return the raw response body. Shared
 /// transport (30s timeout, HTTP-status-as-error) for both calls.
 fn send_chat(key: &str, base_url: &str, payload: &Value) -> Result<String, GroqError> {
     let body = serde_json::to_string(payload).map_err(|e| GroqError::Deserialize(e.to_string()))?;
     let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
     let config = ureq::Agent::config_builder()
         .timeout_global(Some(Duration::from_secs(TIMEOUT_SECS)))
         .http_status_as_error(true)
         .build();
     let agent = ureq::Agent::new_with_config(config);
     let mut response = agent
         .post(&endpoint)
         .header("Authorization", &format!("Bearer {key}"))
         .header("Content-Type", "application/json")
         .send(body.as_str())
         .map_err(map_ureq_error)?;
     response
         .body_mut()
         .read_to_string()
         .map_err(|e| GroqError::Transport(e.to_string()))
 }
 
 /// Extract the first choice's message content (`<think>` stripped, trimmed).
 /// Returns an empty string when there is no content; the caller decides whether
 /// empty is an error.
 fn first_choice_content(raw: &str) -> Result<String, GroqError> {
     let parsed: ChatResponse =
         serde_json::from_str(raw).map_err(|e| GroqError::Deserialize(e.to_string()))?;
     let content = parsed
         .choices
         .into_iter()
         .next()
         .and_then(|c| c.message.content)
         .unwrap_or_default();
     Ok(strip_think(&content).trim().to_string())
 }
 
 /// Generate a single conventional-commit message for the gathered diff via a
 /// direct Groq REST call (FR-10, FR-18). Returns plain text - no JSON plan;
 /// this is the single-commit (tracer/fallback) path.
 pub fn generate_commit_message(diff: &GatheredDiff) -> Result<String, GroqError> {
     let (key, model, base_url) = resolve_config()?;
     let user_content = format!("Diff stats:\n{}\n\nFull diff:\n{}", diff.stat, diff.body);
     let mut payload = json!({
         "model": model,
         "temperature": 0.2,
         "messages": [
             { "role": "system", "content": SYSTEM_PROMPT },
             { "role": "user", "content": user_content },
         ],
     });
     apply_reasoning_suppression(&mut payload, &model);
     let raw = send_chat(&key, &base_url, &payload)?;
     let message = first_choice_content(&raw)?;
     if message.is_empty() {
         return Err(GroqError::EmptyResponse);
     }
     Ok(message)
 }
 
 /// Request a grouping plan via structured outputs (ADR-001 Decisions 1 & 5):
 /// `response_format` json_schema with `strict: true`, deserialized into a typed
 /// [`Plan`]. Grouping-path failures fall back to [`generate_commit_message`].
 pub fn generate_plan(context: &GroupingContext) -> Result<Plan, GroqError> {
     let (key, model, base_url) = resolve_config()?;
     let payload = build_plan_payload(context, &model);
     let raw = send_chat(&key, &base_url, &payload)?;
     let json = first_choice_content(&raw)?;
     if json.is_empty() {
         return Err(GroqError::EmptyResponse);
     }
     serde_json::from_str(&json).map_err(|e| GroqError::Deserialize(e.to_string()))
 }
diff --git a/src/main.rs b/src/main.rs
index 17a9ce1..ff796ba 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,227 +1,275 @@
+mod cache;
 mod cli;
 mod diff;
 mod error;
 mod git;
 mod groq;
 mod plan;
 mod ui;
 
 use std::collections::HashSet;
 
 use clap::Parser;
 
 use cli::Cli;
 use error::GcmError;
 use git::{ChangedFile, Repo};
 use plan::Plan;
 use ui::Decision;
 
 fn main() {
     let args = Cli::parse();
     std::process::exit(run(&args));
 }
 
 /// Returns the process exit code: 0 = success or user abort, 1 = runtime error
 /// (usage errors exit 2 via clap before we get here). See FR-9, FR-39.
 fn run(args: &Cli) -> i32 {
     match execute(args) {
         Ok(()) => 0,
         Err(e) => {
             eprintln!("gcm: {e}");
             e.exit_code()
         }
     }
 }
 
 fn execute(args: &Cli) -> Result<(), GcmError> {
     let repo = Repo::discover()?.ok_or(GcmError::NotARepo)?;
 
+    // `--reset` discards any cached plan up front (FR-8/FR-28), before the
+    // no-changes check so it clears even when the tree is currently clean.
+    if args.reset {
+        cache::clear(&repo);
+    }
+
     if !repo.has_changes()? {
         println!("No changes to commit");
         return Ok(());
     }
 
     // Fail fast before sending any diff to the provider if we could not confirm
     // the commit anyway (ADR-001 #10, AC-11).
     if ui::needs_terminal_but_absent(args.yes, args.dry_run) {
         return Err(GcmError::NonInteractive);
     }
 
     // Merge-state guard (CLO-487 review-2 #2) runs BEFORE any grouping bypass,
     // including `--all`: staging a conflicted working tree on *either* path
     // (grouping `add` or single-commit `add -A`) would bake `<<<<<<<` markers
     // into the commit, so an unresolved conflict must abort regardless of flags.
     let changed = repo.changed_files()?;
     if changed.iter().any(|c| c.is_unmerged()) {
         return Err(GcmError::UnmergedConflicts);
     }
 
     // `--all`, or a clean merge-in-progress, bypasses grouping and commits
     // everything as one. A clean `MERGE_HEAD` makes `git commit` finalize the
-    // merge as a proper two-parent merge commit.
+    // merge as a proper two-parent merge commit. The single-commit path clears
+    // the cached plan (FR-28).
     if args.all || repo.is_merging() {
         return single_commit(&repo, args);
     }
 
-    // Grouping path. A structured-output/parse/validation failure falls back to
-    // the single-commit path with an announced reason (never silent). A fatal
-    // error (missing key, git failure) is returned as-is - the single-commit
-    // path would hit the same wall, so there is nothing to fall back to.
-    let plan = match build_plan(&repo, &changed) {
-        Ok(plan) => plan,
-        Err(BuildError::Fatal(e)) => return Err(e),
-        Err(BuildError::Fallback(reason)) => {
-            eprintln!("gcm: {reason}. Falling back to single-commit mode.");
-            return single_commit(&repo, args);
-        }
+    // Grouping path. A fresh plan is persisted to the per-repo cache; a cache
+    // hit reuses it and skips the grouping call entirely (FR-25/FR-2). The
+    // model is folded into the freshness fingerprint (FR-27). A structured-
+    // output/parse/validation failure falls back to the single-commit path with
+    // an announced reason (never silent); a fatal error (missing key, git
+    // failure) is returned as-is.
+    let model = groq::resolved_model();
+    let plan = match cache::load(&repo, &model) {
+        Some(plan) => plan,
+        None => match build_plan(&repo, &changed) {
+            Ok(plan) => {
+                // Save the full plan even on a `--dry-run` (FR-7: dry-run
+                // uses/saves but does not advance); advancement is gated later.
+                cache::save(&repo, &plan, &model);
+                plan
+            }
+            Err(BuildError::Fatal(e)) => return Err(e),
+            Err(BuildError::Fallback(reason)) => {
+                eprintln!("gcm: {reason}. Falling back to single-commit mode.");
+                return single_commit(&repo, args);
+            }
+        },
     };
 
-    commit_first_group(&repo, args, &changed, &plan)
+    commit_first_group(&repo, args, &changed, &plan, &model)
+}
+
+/// Whether the group-commit flow committed or the user aborted. Gates cache
+/// advancement: only a real commit advances the plan (FR-26) - never an abort.
+#[derive(Debug, PartialEq, Eq)]
+enum CommitOutcome {
+    Committed,
+    Aborted,
 }
 
 /// Outcome of a failed grouping attempt: `Fatal` errors abort (the single-commit
 /// path needs the same resource), `Fallback` errors degrade to single-commit.
 enum BuildError {
     Fatal(GcmError),
     Fallback(String),
 }
 
 /// Gather the grouping context, request the plan, and basic-validate it.
 /// Model/plan failures (structured-output error, unparseable JSON, empty
 /// response, validation) are `Fallback`; a missing key or git failure is
 /// `Fatal`.
 fn build_plan(repo: &Repo, changed: &[ChangedFile]) -> Result<Plan, BuildError> {
     let ctx = diff::gather_for_grouping(repo, changed).map_err(BuildError::Fatal)?;
     let plan = groq::generate_plan(&ctx).map_err(|e| match e {
         // Missing key fails both paths identically; do not pretend to recover.
         groq::GroqError::MissingKey => BuildError::Fatal(GcmError::Groq(e)),
         other => BuildError::Fallback(other.to_string()),
     })?;
     let change_set: HashSet<String> = changed.iter().map(|c| c.path.clone()).collect();
     plan::validate_basic(&plan, &change_set).map_err(|e| BuildError::Fallback(e.to_string()))?;
     Ok(plan)
 }
 
-/// Display the groups, then (unless `--dry-run`) confirm and commit group 1.
+/// Display the groups, then (unless `--dry-run`) confirm and commit group 1,
+/// advancing the cache on a successful commit.
 fn commit_first_group(
     repo: &Repo,
     args: &Cli,
     changed: &[ChangedFile],
     plan: &Plan,
+    model: &str,
 ) -> Result<(), GcmError> {
     display_groups(plan);
-    let group1 = &plan.groups[0]; // validated non-empty with a usable message
-    let message = group1.commit_message.clone().unwrap_or_default();
+    let group1 = &plan.groups[0];
+    let group1_files = select_changed(changed, &group1.files);
+
+    // Resolve group 1's message. A fresh plan (or a full-plan cache hit) already
+    // carries it; an advanced cache hit has a null message, so regenerate it
+    // per group (ADR-001 #6) via a message-only call scoped to this group's diff,
+    // taken BEFORE staging. No grouping call is made here.
+    let message = match group1.commit_message.as_deref() {
+        Some(m) if !m.trim().is_empty() => m.to_string(),
+        _ => groq::generate_commit_message(&diff::gather_for_files(repo, &group1_files)?)?,
+    };
 
     if args.dry_run {
         ui::preview_plan(&message, plan.groups.len(), remaining_files(plan));
         return Ok(());
     }
 
-    let group1_files = select_changed(changed, &group1.files);
-
-    // Capture the pre-run index up front; restore on any post-snapshot failure
-    // (FR-47). Abort never mutates the index, so it needs no restore.
+    // Capture the pre-run index up front. Restore it only on a *pre-commit-step*
+    // failure (FR-47). A commit-step failure (CommitFailed) leaves the group
+    // staged for retry (FR-58), so it is NOT restored. Abort never mutates the
+    // index, so it needs no restore.
     let snapshot = repo.snapshot_index()?;
     let result = commit_group_flow(repo, args, &group1_files, &message);
-    if result.is_err() {
-        let _ = repo.restore_index(&snapshot);
+    if let Err(e) = &result {
+        if !e.leaves_staged() {
+            let _ = repo.restore_index(&snapshot);
+        }
     }
-    result
+
+    // Advance the cache only on a real commit - never on abort or failure.
+    if matches!(&result, Ok(CommitOutcome::Committed)) {
+        cache::advance(repo, plan, model);
+    }
+    result.map(|_| ())
 }
 
 /// Confirm, then clear staging and stage exactly group 1 before committing.
 fn commit_group_flow(
     repo: &Repo,
     args: &Cli,
     group1_files: &[&ChangedFile],
     message: &str,
-) -> Result<(), GcmError> {
+) -> Result<CommitOutcome, GcmError> {
     match ui::confirm(message, args.yes)? {
         Decision::Abort => {
             println!("Aborted. Nothing staged, nothing committed.");
-            Ok(())
+            Ok(CommitOutcome::Aborted)
         }
         Decision::Commit(final_message) => {
             repo.clear_staged()?;
             repo.stage_group(group1_files)?;
             repo.commit_signed(&final_message)?;
             println!("Committed group 1.");
-            Ok(())
+            Ok(CommitOutcome::Committed)
         }
     }
 }
 
 /// The single-commit path (CLO-486 tracer): used by `--all`, a clean
 /// merge-in-progress, and the grouping fallback. Commits all changes as one.
 fn single_commit(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
     if args.dry_run {
         let gathered = diff::gather(repo)?;
         let message = groq::generate_commit_message(&gathered)?;
         ui_preview(&message);
         return Ok(());
     }
+    // `--all`, a clean merge, and the grouping fallback all clear the cached
+    // plan (FR-28); reached only on the real (non-dry-run) path.
+    cache::clear(repo);
     let snapshot = repo.snapshot_index()?;
     let result = single_commit_flow(repo, args);
     if result.is_err() {
         let _ = repo.restore_index(&snapshot);
     }
     result
 }
 
 fn single_commit_flow(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
     let gathered = diff::gather(repo)?;
     let message = groq::generate_commit_message(&gathered)?;
     match ui::confirm(&message, args.yes)? {
         Decision::Abort => {
             println!("Aborted. Nothing staged, nothing committed.");
             Ok(())
         }
         Decision::Commit(final_message) => {
             repo.stage_all()?;
             repo.commit_signed(&final_message)?;
             println!("Committed.");
             Ok(())
         }
     }
 }
 
 /// Resolve group 1's file paths back to their `ChangedFile` entries (so rename
 /// staging can include the original path). Validation guarantees every path
 /// resolves.
 fn select_changed<'a>(changed: &'a [ChangedFile], paths: &[String]) -> Vec<&'a ChangedFile> {
     paths
         .iter()
         .filter_map(|p| changed.iter().find(|c| &c.path == p))
         .collect()
 }
 
 /// Number of files in groups after the first (committed on later runs).
 fn remaining_files(plan: &Plan) -> usize {
     plan.groups.iter().skip(1).map(|g| g.files.len()).sum()
 }
 
 fn display_groups(plan: &Plan) {
     println!();
     println!("Found {} group(s):", plan.groups.len());
     for (i, group) in plan.groups.iter().enumerate() {
         println!();
         if i == 0 {
             println!("> Group 1 (committing now): {}", group.summary);
         } else {
             println!("  Group {} (next run): {}", i + 1, group.summary);
         }
         for file in &group.files {
             println!("    {file}");
         }
     }
     println!();
 }
 
 fn ui_preview(message: &str) {
     println!();
     println!("Commit message (dry run - nothing staged or committed):");
     println!("-----------------------------");
     println!("{message}");
     println!("-----------------------------");
 }
diff --git a/src/plan.rs b/src/plan.rs
index ca6afb5..c408168 100644
--- a/src/plan.rs
+++ b/src/plan.rs
@@ -1,97 +1,99 @@
 use std::collections::HashSet;
 use std::fmt;
 
-use serde::Deserialize;
+use serde::{Deserialize, Serialize};
 use serde_json::{json, Value};
 
 /// The grouping plan returned by the provider's structured-output mode
 /// (ADR-001 Decision 1). Typed deserialization replaces the bash tool's
 /// `sed -> perl -> jq` scrape of reasoning-polluted JSON (FR-16, FR-19).
-#[derive(Debug, Deserialize)]
+/// `Serialize`/`Clone` so the plan can be persisted to (and advanced in) the
+/// per-repo cache (CLO-491, FR-25).
+#[derive(Debug, Clone, Deserialize, Serialize)]
 pub struct Plan {
     pub groups: Vec<Group>,
 }
 
 /// One logical commit: the files it covers, a one-line summary, and (for
 /// `groups[0]` only, per the regenerate-per-group contract) a commit message.
-#[derive(Debug, Deserialize)]
+#[derive(Debug, Clone, Deserialize, Serialize)]
 pub struct Group {
     pub files: Vec<String>,
     pub summary: String,
     /// Full conventional-commit message for `groups[0]`; `null` for later
     /// groups (we re-analyze each run, so their messages are never used here).
     pub commit_message: Option<String>,
 }
 
 /// Why a plan was rejected by [`validate_basic`]. Each maps to an announced
 /// fallback to the single-commit path (FR-23 basic; full validation is CLO-492).
 #[derive(Debug, PartialEq, Eq)]
 pub enum PlanError {
     /// The plan has no groups at all (`groups: []`).
     NoGroups,
     /// Group 1 references no files - nothing to commit.
     EmptyFirstGroup,
     /// Group 1 has a null/empty commit message (the exact bash null-message bug).
     MissingFirstMessage,
     /// A plan file is not in the real change set (a hallucinated path).
     UnknownFile(String),
 }
 
 impl fmt::Display for PlanError {
     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
         match self {
             PlanError::NoGroups => write!(f, "plan contained no groups"),
             PlanError::EmptyFirstGroup => write!(f, "group 1 references no files"),
             PlanError::MissingFirstMessage => {
                 write!(f, "group 1 has no commit message")
             }
             PlanError::UnknownFile(p) => {
                 write!(f, "group 1 references unknown file '{p}'")
             }
         }
     }
 }
 
 /// The inner JSON Schema object sent with `response_format` (ADR-001 Decision 5,
 /// Groq strict mode): every property is `required` and every object sets
 /// `additionalProperties: false`; `commit_message` is nullable so later groups
 /// can carry `null`.
 pub fn schema() -> Value {
     json!({
         "type": "object",
         "properties": {
             "groups": {
                 "type": "array",
                 "items": {
                     "type": "object",
                     "properties": {
                         "files": { "type": "array", "items": { "type": "string" } },
                         "summary": { "type": "string" },
                         "commit_message": { "type": ["string", "null"] }
                     },
                     "required": ["files", "summary", "commit_message"],
                     "additionalProperties": false
                 }
             }
         },
         "required": ["groups"],
         "additionalProperties": false
     })
 }
 
 /// Basic plan validation (FR-23 basic): the plan must have at least one group,
 /// group 1 must be non-empty and carry a usable message, and no group may
 /// reference a file absent from the real change set. Full bijective validation
 /// (every changed file covered exactly once) is CLO-492.
 pub fn validate_basic(plan: &Plan, change_set: &HashSet<String>) -> Result<(), PlanError> {
     let first = plan.groups.first().ok_or(PlanError::NoGroups)?;
     if first.files.is_empty() {
         return Err(PlanError::EmptyFirstGroup);
     }
     match &first.commit_message {
         Some(m) if !m.trim().is_empty() => {}
         _ => return Err(PlanError::MissingFirstMessage),
     }
     // No group may reference a file outside the real change set (catches
     // hallucinated paths). Full coverage/bijection checks are CLO-492.
     for group in &plan.groups {

codex
The cache module and `main.rs` wiring are in view. I’m filling in the remaining modules now, especially `diff.rs` and `git.rs`, because the review hinges on path-scoped message diffs and commit-failure index behavior.
exec
/bin/zsh -lc "sed -n '1,260p' src/main.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
mod cache;
mod cli;
mod diff;
mod error;
mod git;
mod groq;
mod plan;
mod ui;

use std::collections::HashSet;

use clap::Parser;

use cli::Cli;
use error::GcmError;
use git::{ChangedFile, Repo};
use plan::Plan;
use ui::Decision;

fn main() {
    let args = Cli::parse();
    std::process::exit(run(&args));
}

/// Returns the process exit code: 0 = success or user abort, 1 = runtime error
/// (usage errors exit 2 via clap before we get here). See FR-9, FR-39.
fn run(args: &Cli) -> i32 {
    match execute(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("gcm: {e}");
            e.exit_code()
        }
    }
}

fn execute(args: &Cli) -> Result<(), GcmError> {
    let repo = Repo::discover()?.ok_or(GcmError::NotARepo)?;

    // `--reset` discards any cached plan up front (FR-8/FR-28), before the
    // no-changes check so it clears even when the tree is currently clean.
    if args.reset {
        cache::clear(&repo);
    }

    if !repo.has_changes()? {
        println!("No changes to commit");
        return Ok(());
    }

    // Fail fast before sending any diff to the provider if we could not confirm
    // the commit anyway (ADR-001 #10, AC-11).
    if ui::needs_terminal_but_absent(args.yes, args.dry_run) {
        return Err(GcmError::NonInteractive);
    }

    // Merge-state guard (CLO-487 review-2 #2) runs BEFORE any grouping bypass,
    // including `--all`: staging a conflicted working tree on *either* path
    // (grouping `add` or single-commit `add -A`) would bake `<<<<<<<` markers
    // into the commit, so an unresolved conflict must abort regardless of flags.
    let changed = repo.changed_files()?;
    if changed.iter().any(|c| c.is_unmerged()) {
        return Err(GcmError::UnmergedConflicts);
    }

    // `--all`, or a clean merge-in-progress, bypasses grouping and commits
    // everything as one. A clean `MERGE_HEAD` makes `git commit` finalize the
    // merge as a proper two-parent merge commit. The single-commit path clears
    // the cached plan (FR-28).
    if args.all || repo.is_merging() {
        return single_commit(&repo, args);
    }

    // Grouping path. A fresh plan is persisted to the per-repo cache; a cache
    // hit reuses it and skips the grouping call entirely (FR-25/FR-2). The
    // model is folded into the freshness fingerprint (FR-27). A structured-
    // output/parse/validation failure falls back to the single-commit path with
    // an announced reason (never silent); a fatal error (missing key, git
    // failure) is returned as-is.
    let model = groq::resolved_model();
    let plan = match cache::load(&repo, &model) {
        Some(plan) => plan,
        None => match build_plan(&repo, &changed) {
            Ok(plan) => {
                // Save the full plan even on a `--dry-run` (FR-7: dry-run
                // uses/saves but does not advance); advancement is gated later.
                cache::save(&repo, &plan, &model);
                plan
            }
            Err(BuildError::Fatal(e)) => return Err(e),
            Err(BuildError::Fallback(reason)) => {
                eprintln!("gcm: {reason}. Falling back to single-commit mode.");
                return single_commit(&repo, args);
            }
        },
    };

    commit_first_group(&repo, args, &changed, &plan, &model)
}

/// Whether the group-commit flow committed or the user aborted. Gates cache
/// advancement: only a real commit advances the plan (FR-26) - never an abort.
#[derive(Debug, PartialEq, Eq)]
enum CommitOutcome {
    Committed,
    Aborted,
}

/// Outcome of a failed grouping attempt: `Fatal` errors abort (the single-commit
/// path needs the same resource), `Fallback` errors degrade to single-commit.
enum BuildError {
    Fatal(GcmError),
    Fallback(String),
}

/// Gather the grouping context, request the plan, and basic-validate it.
/// Model/plan failures (structured-output error, unparseable JSON, empty
/// response, validation) are `Fallback`; a missing key or git failure is
/// `Fatal`.
fn build_plan(repo: &Repo, changed: &[ChangedFile]) -> Result<Plan, BuildError> {
    let ctx = diff::gather_for_grouping(repo, changed).map_err(BuildError::Fatal)?;
    let plan = groq::generate_plan(&ctx).map_err(|e| match e {
        // Missing key fails both paths identically; do not pretend to recover.
        groq::GroqError::MissingKey => BuildError::Fatal(GcmError::Groq(e)),
        other => BuildError::Fallback(other.to_string()),
    })?;
    let change_set: HashSet<String> = changed.iter().map(|c| c.path.clone()).collect();
    plan::validate_basic(&plan, &change_set).map_err(|e| BuildError::Fallback(e.to_string()))?;
    Ok(plan)
}

/// Display the groups, then (unless `--dry-run`) confirm and commit group 1,
/// advancing the cache on a successful commit.
fn commit_first_group(
    repo: &Repo,
    args: &Cli,
    changed: &[ChangedFile],
    plan: &Plan,
    model: &str,
) -> Result<(), GcmError> {
    display_groups(plan);
    let group1 = &plan.groups[0];
    let group1_files = select_changed(changed, &group1.files);

    // Resolve group 1's message. A fresh plan (or a full-plan cache hit) already
    // carries it; an advanced cache hit has a null message, so regenerate it
    // per group (ADR-001 #6) via a message-only call scoped to this group's diff,
    // taken BEFORE staging. No grouping call is made here.
    let message = match group1.commit_message.as_deref() {
        Some(m) if !m.trim().is_empty() => m.to_string(),
        _ => groq::generate_commit_message(&diff::gather_for_files(repo, &group1_files)?)?,
    };

    if args.dry_run {
        ui::preview_plan(&message, plan.groups.len(), remaining_files(plan));
        return Ok(());
    }

    // Capture the pre-run index up front. Restore it only on a *pre-commit-step*
    // failure (FR-47). A commit-step failure (CommitFailed) leaves the group
    // staged for retry (FR-58), so it is NOT restored. Abort never mutates the
    // index, so it needs no restore.
    let snapshot = repo.snapshot_index()?;
    let result = commit_group_flow(repo, args, &group1_files, &message);
    if let Err(e) = &result {
        if !e.leaves_staged() {
            let _ = repo.restore_index(&snapshot);
        }
    }

    // Advance the cache only on a real commit - never on abort or failure.
    if matches!(&result, Ok(CommitOutcome::Committed)) {
        cache::advance(repo, plan, model);
    }
    result.map(|_| ())
}

/// Confirm, then clear staging and stage exactly group 1 before committing.
fn commit_group_flow(
    repo: &Repo,
    args: &Cli,
    group1_files: &[&ChangedFile],
    message: &str,
) -> Result<CommitOutcome, GcmError> {
    match ui::confirm(message, args.yes)? {
        Decision::Abort => {
            println!("Aborted. Nothing staged, nothing committed.");
            Ok(CommitOutcome::Aborted)
        }
        Decision::Commit(final_message) => {
            repo.clear_staged()?;
            repo.stage_group(group1_files)?;
            repo.commit_signed(&final_message)?;
            println!("Committed group 1.");
            Ok(CommitOutcome::Committed)
        }
    }
}

/// The single-commit path (CLO-486 tracer): used by `--all`, a clean
/// merge-in-progress, and the grouping fallback. Commits all changes as one.
fn single_commit(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
    if args.dry_run {
        let gathered = diff::gather(repo)?;
        let message = groq::generate_commit_message(&gathered)?;
        ui_preview(&message);
        return Ok(());
    }
    // `--all`, a clean merge, and the grouping fallback all clear the cached
    // plan (FR-28); reached only on the real (non-dry-run) path.
    cache::clear(repo);
    let snapshot = repo.snapshot_index()?;
    let result = single_commit_flow(repo, args);
    if result.is_err() {
        let _ = repo.restore_index(&snapshot);
    }
    result
}

fn single_commit_flow(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
    let gathered = diff::gather(repo)?;
    let message = groq::generate_commit_message(&gathered)?;
    match ui::confirm(&message, args.yes)? {
        Decision::Abort => {
            println!("Aborted. Nothing staged, nothing committed.");
            Ok(())
        }
        Decision::Commit(final_message) => {
            repo.stage_all()?;
            repo.commit_signed(&final_message)?;
            println!("Committed.");
            Ok(())
        }
    }
}

/// Resolve group 1's file paths back to their `ChangedFile` entries (so rename
/// staging can include the original path). Validation guarantees every path
/// resolves.
fn select_changed<'a>(changed: &'a [ChangedFile], paths: &[String]) -> Vec<&'a ChangedFile> {
    paths
        .iter()
        .filter_map(|p| changed.iter().find(|c| &c.path == p))
        .collect()
}

/// Number of files in groups after the first (committed on later runs).
fn remaining_files(plan: &Plan) -> usize {
    plan.groups.iter().skip(1).map(|g| g.files.len()).sum()
}

fn display_groups(plan: &Plan) {
    println!();
    println!("Found {} group(s):", plan.groups.len());
    for (i, group) in plan.groups.iter().enumerate() {
        println!();
        if i == 0 {
            println!("> Group 1 (committing now): {}", group.summary);
        } else {
            println!("  Group {} (next run): {}", i + 1, group.summary);

exec
/bin/zsh -lc "sed -n '1,280p' src/diff.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use crate::error::GcmError;
use crate::git::{ChangedFile, Repo};

/// JSON-encode the changed-file paths as an array of strings so a path
/// containing a newline (or any character) stays a single discrete element in
/// the grouping prompt - newline-joining would split such a path into multiple
/// lines and the model would group phantom paths (CLO-487 path-agreement).
fn file_list_json(changed: &[ChangedFile]) -> String {
    let paths: Vec<&str> = changed.iter().map(|c| c.path.as_str()).collect();
    serde_json::to_string(&paths).unwrap_or_else(|_| "[]".to_string())
}

/// JSON-encode the porcelain status as an array of `"XY path"` strings (also
/// newline-safe, same rationale as [`file_list_json`]).
fn status_json(changed: &[ChangedFile]) -> String {
    let rows: Vec<String> = changed
        .iter()
        .map(|c| format!("{}{} {}", c.x as char, c.y as char, c.path))
        .collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
}

/// Untracked-expansion caps (FR-57): bound both file count and total bytes so an
/// un-ignored directory of thousands of files cannot freeze the CLI.
const MAX_UNTRACKED_FILES: usize = 50;
const MAX_UNTRACKED_BYTES: usize = 256 * 1024;
/// Per-file read cap for an individual untracked file (mirrors bash `head -c 8192`).
const PER_FILE_BYTES: usize = 8192;
/// Per-file cap for a tracked diff section in the grouping prompt: each file's
/// section is truncated independently with a `[diff omitted: N bytes]`
/// placeholder rather than tail-chopping the whole body (CLO-487 FR-15).
const PER_FILE_DIFF_BYTES: usize = 8192;
/// Coarse final safeguard on the whole assembled body.
const MAX_TOTAL_BYTES: usize = 350_000;

/// The diff context handed to the provider.
pub struct GatheredDiff {
    pub stat: String,
    pub body: String,
}

/// The richer context handed to the provider for grouping (CLO-487): the file
/// list and porcelain status (both JSON arrays, so newline-containing paths stay
/// discrete), the diff `--stat`, and the per-file-truncated full diff. Distinct
/// from [`GatheredDiff`] to keep the tracer's single-message concerns separate.
pub struct GroupingContext {
    /// JSON array of the exact changed paths (the model groups by these).
    pub file_list: String,
    /// JSON array of `"XY path"` porcelain status rows.
    pub status: String,
    pub stat: String,
    pub body: String,
}

/// Build the prompt diff: tracked changes (binary-elided) plus untracked,
/// non-gitignored file content, bounded by the FR-57 caps. Reads only the
/// working tree; nothing is staged (FR-47).
pub fn gather(repo: &Repo) -> Result<GatheredDiff, GcmError> {
    let stat = repo.diff_stat()?;
    let tracked = repo.diff_full()?;
    let mut body = elide_binary_diff(&tracked);
    append_untracked(repo, &mut body, None)?;
    cap_total(&mut body);
    Ok(GatheredDiff { stat, body })
}

/// Build the single-message diff for **one commit group** (CLO-491, FR-45): the
/// tracked diff and stat scoped to the group's paths, plus the group's own
/// untracked files (filtered, so other groups' untracked content never leaks
/// into this message). Used to regenerate a message-only call for an advanced
/// group on a cache hit. Unborn-safe: with no `HEAD` the tracked diff is empty
/// and all content arrives through the filtered untracked path.
pub fn gather_for_files(repo: &Repo, files: &[&ChangedFile]) -> Result<GatheredDiff, GcmError> {
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let stat = repo.diff_stat_for(&paths)?;
    let tracked = repo.diff_full_for(&paths)?;
    let mut body = elide_binary_diff(&tracked);
    let allow: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
    append_untracked(repo, &mut body, Some(&allow))?;
    cap_total(&mut body);
    Ok(GatheredDiff { stat, body })
}

/// Build the grouping context (CLO-487): the file list and porcelain status are
/// derived from the already-gathered `changed` set (so they stay byte-identical
/// to the paths used for validation and staging), the diff `--stat` is the
/// prompt header, and the body is the tracked diff truncated **per file** with
/// `[diff omitted: N bytes]` placeholders, plus untracked content (FR-57 caps),
/// under the `MAX_TOTAL_BYTES` final safeguard.
pub fn gather_for_grouping(
    repo: &Repo,
    changed: &[ChangedFile],
) -> Result<GroupingContext, GcmError> {
    let file_list = file_list_json(changed);
    let status = status_json(changed);

    let stat = repo.diff_stat()?;
    let tracked = repo.diff_full()?;
    let mut body = truncate_per_file(&elide_binary_diff(&tracked), PER_FILE_DIFF_BYTES);
    append_untracked(repo, &mut body, None)?;
    cap_total(&mut body);

    Ok(GroupingContext {
        file_list,
        status,
        stat,
        body,
    })
}

/// Append untracked, non-gitignored file content to `body`, bounded by the
/// FR-57 file-count and byte caps. Shared by [`gather`] and
/// [`gather_for_grouping`] (which pass `None` = every untracked file) and
/// [`gather_for_files`] (which passes `Some(allow)` to restrict to one group's
/// paths, so a single group's message diff is not polluted by other groups'
/// untracked files - CLO-491).
fn append_untracked(
    repo: &Repo,
    body: &mut String,
    allow: Option<&HashSet<String>>,
) -> Result<(), GcmError> {
    let mut untracked = repo.untracked_files()?;
    untracked.sort();

    // Every (allow-listed) untracked path counts toward the file-count cap -
    // binary and unreadable files included - so a directory of thousands of
    // files (of any kind) cannot force thousands of reads. Once either cap is
    // reached, every remaining file is listed by name only, with no read at all
    // (FR-57).
    let mut files_done = 0usize;
    let mut bytes_used = 0usize;
    for path in &untracked {
        // Filter to the allow-list (if any) before the caps, so excluded paths
        // neither consume the budget nor reach the prompt.
        if allow.is_some_and(|a| !a.contains(path)) {
            continue;
        }
        if files_done >= MAX_UNTRACKED_FILES || bytes_used >= MAX_UNTRACKED_BYTES {
            body.push_str(&format!(
                "\n--- /dev/null\n+++ b/{path}\n[content omitted: untracked cap reached]\n"
            ));
            continue;
        }
        let full = repo.root().join(path);
        // Only read regular files. `symlink_metadata` does not follow symlinks,
        // so we never read a symlink's target (which could leak content from
        // outside the repo) and never block on a FIFO/device/socket.
        let is_regular = std::fs::symlink_metadata(&full)
            .map(|m| m.file_type().is_file())
            .unwrap_or(false);
        if !is_regular {
            body.push_str(&format!(
                "\n--- /dev/null\n+++ b/{path}\n[omitted: not a regular file]\n"
            ));
            files_done += 1;
            continue;
        }
        // Read at most a per-file slice bounded by the remaining byte budget, so
        // a single huge file is never loaded into memory in full.
        let budget = (MAX_UNTRACKED_BYTES - bytes_used).min(PER_FILE_BYTES);
        match read_capped(&full, budget) {
            Ok((content, more)) if looks_binary(&content) => {
                body.push_str(&format!("\n--- /dev/null\n+++ b/{path}\n+[binary file]\n"));
                let _ = more;
            }
            Ok((content, more)) => {
                let text = String::from_utf8_lossy(&content);
                body.push_str(&format!("\n--- /dev/null\n+++ b/{path}\n"));
                for line in text.lines() {
                    body.push('+');
                    body.push_str(line);
                    body.push('\n');
                }
                if more {
                    body.push_str("+[truncated]\n");
                }
                bytes_used += content.len();
            }
            Err(_) => {
                // Unreadable (perm, race, symlink loop) - note by name, never block.
                body.push_str(&format!(
                    "\n--- /dev/null\n+++ b/{path}\n[omitted: unreadable]\n"
                ));
            }
        }
        files_done += 1;
    }
    Ok(())
}

/// Coarse final safeguard on the whole assembled body (FR-57), truncating on a
/// char boundary so a multibyte char split at the cap does not panic.
fn cap_total(body: &mut String) {
    if body.len() > MAX_TOTAL_BYTES {
        let mut end = MAX_TOTAL_BYTES;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("\n... (diff truncated)\n");
    }
}

/// Truncate a tracked diff **per file**: split on `diff --git ` boundaries and,
/// for any section longer than `cap`, keep the file's header and replace its
/// hunk body with `[diff omitted: N bytes]` (N = omitted bytes). This keeps
/// every changed file present in the prompt instead of tail-chopping the whole
/// body and severing the last file mid-hunk (CLO-487 FR-15).
fn truncate_per_file(diff: &str, cap: usize) -> String {
    let mut out = String::new();
    let mut section = String::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") && !section.is_empty() {
            push_capped_section(&section, cap, &mut out);
            section.clear();
        }
        section.push_str(line);
    }
    if !section.is_empty() {
        push_capped_section(&section, cap, &mut out);
    }
    out
}

fn push_capped_section(section: &str, cap: usize, out: &mut String) {
    if section.len() <= cap {
        out.push_str(section);
        return;
    }
    // Keep the header (lines up to the first hunk `@@`); if there is no hunk
    // marker, keep just the first line. Replace the rest with a byte-count
    // placeholder.
    let mut header_end = None;
    let mut idx = 0;
    let mut first_line_end = section.len();
    for (i, line) in section.split_inclusive('\n').enumerate() {
        if i == 0 {
            first_line_end = line.len();
        }
        if line.starts_with("@@") {
            header_end = Some(idx);
            break;
        }
        idx += line.len();
    }
    let header = &section[..header_end.unwrap_or(first_line_end)];
    let omitted = section.len() - header.len();
    out.push_str(header);
    if !header.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!("[diff omitted: {omitted} bytes]\n"));
}

/// Read at most `cap` bytes from a file without loading it fully into memory.
/// Returns the bytes and whether the file had more content beyond `cap`.
fn read_capped(path: &Path, cap: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let file = std::fs::File::open(path)?;
    // Read one extra byte so we can tell whether the file exceeded the cap.
    let mut buf = Vec::new();
    file.take(cap as u64 + 1).read_to_end(&mut buf)?;
    let more = buf.len() > cap;
    buf.truncate(cap);
    Ok((buf, more))
}

/// Heuristic: is this byte sample binary? NUL bytes or invalid UTF-8 (beyond a
/// possible multibyte char split at the sample boundary) mean binary. UTF-8 text
/// (including non-ASCII) is preserved (FR-32, NUL-misclassification guard).
fn looks_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return true;
    }
    match std::str::from_utf8(sample) {
        Ok(_) => false,

exec
/bin/zsh -lc "sed -n '1,160p' src/cli.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
use clap::Parser;

/// Build-stamped version: crate version plus the git short SHA from build.rs (AC-1).
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("GCM_GIT_SHA"));

const EGRESS_DISCLOSURE: &str = "\
gcm groups your working-tree changes into logical commits and commits the first group;\n\
run it again to commit the next group. Grouping operates on whole files over the entire\n\
working tree, so it overrides any manual hunk-level (git add -p) staging: group 1's files\n\
are staged in full, later groups are left unstaged (their changes are never lost).\n\
\n\
PRIVACY: gcm sends your working-tree diff and the content of untracked, non-gitignored\n\
files to the configured LLM provider (Groq) to generate the plan and commit messages.\n\
Gitignored files (e.g. .env) are never sent. See the README for each provider's data policy.";

#[derive(Parser, Debug)]
#[command(
    name = "gcm",
    version = VERSION,
    about = "Generate one signed conventional-commit from your working-tree changes via Groq.",
    after_help = EGRESS_DISCLOSURE,
    after_long_help = EGRESS_DISCLOSURE
)]
pub struct Cli {
    /// Preview the grouping plan (or the single-commit message with --all) and
    /// exit without staging or committing.
    #[arg(long)]
    pub dry_run: bool,

    /// Skip grouping and commit all changes as a single commit.
    #[arg(long)]
    pub all: bool,

    /// Discard any cached grouping plan and re-analyze from scratch.
    #[arg(long)]
    pub reset: bool,

    /// Auto-confirm the commit without prompting (for non-interactive / agent / CI use).
    #[arg(long, visible_alias = "no-input")]
    pub yes: bool,
}

exec
/bin/zsh -lc "sed -n '1,260p' src/groq.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
use std::fmt;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::diff::{GatheredDiff, GroupingContext};
use crate::plan::Plan;

const DEFAULT_MODEL: &str = "openai/gpt-oss-120b";
const DEFAULT_BASE_URL: &str = "https://api.groq.com/openai/v1";
const TIMEOUT_SECS: u64 = 30;

const SYSTEM_PROMPT: &str = "\
Analyze this git diff and generate a concise, conventional commit message.
Use format: <type>(<scope>): <description>
Types: feat, fix, docs, style, refactor, test, chore
Keep the first line under 72 characters.
Add a blank line and bullet points for details if there are multiple significant changes.
Do NOT include any explanation - output ONLY the commit message.";

/// System prompt for the grouping plan (CLO-487; adapted from the bash tool,
/// `docs/tmp/git-commit-ai.sh:305-322`). The `response_format` json_schema
/// enforces the output shape, so the prompt carries only the grouping rules.
const GROUPING_SYSTEM_PROMPT: &str = "\
Analyze these git changes. Group related files into logical commits by semantic relevance.

Rules:
- Every file from the file list must appear in exactly one group.
- Prefer fewer groups (1-3) unless changes are truly unrelated.
- commit_message: a full conventional-commit message for groups[0] ONLY; null for every other group.
- Conventional format <type>(<scope>): <description>, first line under 72 chars; add a blank line
  and bullet points for details when there are multiple significant changes.
- For renamed files, use the NEW path in your file list.
- summary: a one-line description of each group.";

/// Errors from the Groq message call. A light taxonomy for the tracer; the full
/// typed-error/retry surface (FR-21/22) lands in CLO-488.
#[derive(Debug)]
pub enum GroqError {
    MissingKey,
    Http(u16),
    Timeout,
    Transport(String),
    EmptyResponse,
    Deserialize(String),
}

impl fmt::Display for GroqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroqError::MissingKey => write!(
                f,
                "GROQ_API_KEY is not set. Export it (e.g. `export GROQ_API_KEY=...`) and retry."
            ),
            GroqError::Http(code) => write!(f, "Groq API returned HTTP {code}"),
            GroqError::Timeout => write!(f, "Groq API request timed out after {TIMEOUT_SECS}s"),
            GroqError::Transport(msg) => write!(f, "could not reach the Groq API: {msg}"),
            GroqError::EmptyResponse => write!(f, "Groq returned an empty commit message"),
            GroqError::Deserialize(msg) => write!(f, "could not parse the Groq response: {msg}"),
        }
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
}

/// The configured model id (`GCM_GROQ_MODEL` or the default), resolved
/// **without** requiring `GROQ_API_KEY`. Used by the plan cache to fold the
/// model into the freshness fingerprint (CLO-491, FR-27) even when no key is set.
pub fn resolved_model() -> String {
    std::env::var("GCM_GROQ_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

/// Resolve `(api_key, model, base_url)` from the environment - shared by the
/// message (tracer) and plan (grouping) calls.
fn resolve_config() -> Result<(String, String, String), GroqError> {
    let key = std::env::var("GROQ_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or(GroqError::MissingKey)?;
    let model = resolved_model();
    let base_url = std::env::var("GCM_GROQ_BASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    Ok((key, model, base_url))
}

/// POST a chat-completions payload and return the raw response body. Shared
/// transport (30s timeout, HTTP-status-as-error) for both calls.
fn send_chat(key: &str, base_url: &str, payload: &Value) -> Result<String, GroqError> {
    let body = serde_json::to_string(payload).map_err(|e| GroqError::Deserialize(e.to_string()))?;
    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(TIMEOUT_SECS)))
        .http_status_as_error(true)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent
        .post(&endpoint)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send(body.as_str())
        .map_err(map_ureq_error)?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| GroqError::Transport(e.to_string()))
}

/// Extract the first choice's message content (`<think>` stripped, trimmed).
/// Returns an empty string when there is no content; the caller decides whether
/// empty is an error.
fn first_choice_content(raw: &str) -> Result<String, GroqError> {
    let parsed: ChatResponse =
        serde_json::from_str(raw).map_err(|e| GroqError::Deserialize(e.to_string()))?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();
    Ok(strip_think(&content).trim().to_string())
}

/// Generate a single conventional-commit message for the gathered diff via a
/// direct Groq REST call (FR-10, FR-18). Returns plain text - no JSON plan;
/// this is the single-commit (tracer/fallback) path.
pub fn generate_commit_message(diff: &GatheredDiff) -> Result<String, GroqError> {
    let (key, model, base_url) = resolve_config()?;
    let user_content = format!("Diff stats:\n{}\n\nFull diff:\n{}", diff.stat, diff.body);
    let mut payload = json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_content },
        ],
    });
    apply_reasoning_suppression(&mut payload, &model);
    let raw = send_chat(&key, &base_url, &payload)?;
    let message = first_choice_content(&raw)?;
    if message.is_empty() {
        return Err(GroqError::EmptyResponse);
    }
    Ok(message)
}

/// Request a grouping plan via structured outputs (ADR-001 Decisions 1 & 5):
/// `response_format` json_schema with `strict: true`, deserialized into a typed
/// [`Plan`]. Grouping-path failures fall back to [`generate_commit_message`].
pub fn generate_plan(context: &GroupingContext) -> Result<Plan, GroqError> {
    let (key, model, base_url) = resolve_config()?;
    let payload = build_plan_payload(context, &model);
    let raw = send_chat(&key, &base_url, &payload)?;
    let json = first_choice_content(&raw)?;
    if json.is_empty() {
        return Err(GroqError::EmptyResponse);
    }
    serde_json::from_str(&json).map_err(|e| GroqError::Deserialize(e.to_string()))
}

/// Build the structured-output plan request payload (extracted for testing the
/// contract shape without a network call).
fn build_plan_payload(context: &GroupingContext, model: &str) -> Value {
    let user_content = format!(
        "Changed files (JSON array of exact paths - group by these):\n{}\n\n\
         Git status (JSON array of \"XY path\"):\n{}\n\nDiff stats:\n{}\n\nFull diff:\n{}",
        context.file_list, context.status, context.stat, context.body
    );
    let mut payload = json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": GROUPING_SYSTEM_PROMPT },
            { "role": "user", "content": user_content },
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "commit_plan",
                "strict": true,
                "schema": crate::plan::schema(),
            }
        }
    });
    apply_reasoning_suppression(&mut payload, model);
    payload
}

/// Select reasoning-suppression params by model family so chain-of-thought never
/// reaches the message (ADR-001 #5; capability matrix). `<think>` stripping is the
/// universal backstop applied to the response regardless.
fn apply_reasoning_suppression(payload: &mut serde_json::Value, model: &str) {
    let obj = payload.as_object_mut().expect("payload is a JSON object");
    if model.contains("qwen") {
        obj.insert("reasoning_effort".into(), json!("none"));
    } else if model.contains("gpt-oss") {
        obj.insert("include_reasoning".into(), json!(false));
    }
}

fn map_ureq_error(err: ureq::Error) -> GroqError {
    match err {
        ureq::Error::StatusCode(code) => GroqError::Http(code),
        ureq::Error::Timeout(_) => GroqError::Timeout,
        ureq::Error::HostNotFound => GroqError::Transport("host not found".to_string()),
        ureq::Error::Io(e) => GroqError::Transport(e.to_string()),
        other => GroqError::Transport(other.to_string()),
    }
}

/// Remove any `<think>...</think>` spans (reasoning models that only hide rather
/// than disable CoT). Drops an unterminated trailing `<think>` as well.
fn strip_think(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_think_block() {
        let s = "<think>reasoning here</think>feat: add thing";
        assert_eq!(strip_think(s).trim(), "feat: add thing");
    }

    #[test]
    fn strips_multiple_think_blocks() {
        let s = "<think>a</think>fix: x\n<think>b</think>";

exec
/bin/zsh -lc "sed -n '1,220p' src/error.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
use std::fmt;

use crate::groq::GroqError;

/// Top-level runtime error. CLI usage errors are handled by clap (exit 2);
/// every variant here maps to exit code 1. User abort is not an error and is
/// represented as a successful `Outcome`, not a `GcmError`.
#[derive(Debug)]
pub enum GcmError {
    NotARepo,
    Git(String),
    Groq(GroqError),
    /// Non-TTY context without `--yes`/`--no-input`: cannot prompt (ADR-001 #10).
    NonInteractive,
    Editor(String),
    EmptyMessage,
    /// The repository has unresolved merge conflicts (unmerged index entries).
    /// gcm aborts rather than risk committing conflict markers (CLO-487).
    UnmergedConflicts,
    /// `git commit` itself failed after the group was staged (e.g. a rejecting
    /// pre-commit hook, a signing failure). The group is left **staged** and the
    /// plan cache is **not** advanced so the user can fix and retry (CLO-491,
    /// FR-58). Distinct from [`GcmError::Git`] (pre-commit-step failures, which
    /// restore the index).
    CommitFailed(String),
}

impl GcmError {
    /// Process exit code for this error. All runtime errors are 1; usage (exit 2)
    /// is produced by clap before we get here.
    pub fn exit_code(&self) -> i32 {
        1
    }

    /// Whether this error means the staged group should be **left in place**.
    /// Only a commit-step failure ([`GcmError::CommitFailed`]) leaves the group
    /// staged (FR-58); every other error restores the pre-run index (FR-47).
    pub fn leaves_staged(&self) -> bool {
        matches!(self, GcmError::CommitFailed(_))
    }
}

impl fmt::Display for GcmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GcmError::NotARepo => {
                write!(f, "not a git repository (run gcm inside a git work tree)")
            }
            GcmError::Git(msg) => write!(f, "{msg}"),
            GcmError::Groq(e) => write!(f, "{e}"),
            GcmError::NonInteractive => write!(
                f,
                "no terminal available to confirm the commit. Re-run with --yes (or --no-input) \
                 to auto-confirm, or --dry-run to preview without committing."
            ),
            GcmError::Editor(msg) => write!(f, "editor failed: {msg}"),
            GcmError::EmptyMessage => write!(f, "commit message is empty; nothing committed"),
            GcmError::UnmergedConflicts => write!(
                f,
                "repository has unresolved merge conflicts; resolve them and stage your \
                 resolution before running gcm"
            ),
            GcmError::CommitFailed(msg) => write!(
                f,
                "{msg}\nThe group is left staged and the plan was not advanced; \
                 fix the issue and re-run gcm to retry this group."
            ),
        }
    }
}

impl From<GroqError> for GcmError {
    fn from(e: GroqError) -> Self {
        GcmError::Groq(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_commit_failed_leaves_the_group_staged() {
        // FR-58: a commit-step failure leaves the group staged; every other
        // error restores the pre-run index (FR-47).
        assert!(GcmError::CommitFailed("hook rejected".to_string()).leaves_staged());
        assert!(!GcmError::Git("git add failed".to_string()).leaves_staged());
        assert!(!GcmError::UnmergedConflicts.leaves_staged());
        assert!(!GcmError::NotARepo.leaves_staged());
    }

    #[test]
    fn commit_failed_surfaces_the_underlying_error() {
        let msg =
            GcmError::CommitFailed("git commit failed (see output above)".to_string()).to_string();
        assert!(msg.contains("git commit failed"));
        assert!(
            msg.contains("left staged"),
            "tells the user the group is kept"
        );
    }
}

exec
/bin/zsh -lc "sed -n '1,260p' src/git.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
exec
/bin/zsh -lc "sed -n '1,220p' src/plan.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The grouping plan returned by the provider's structured-output mode
/// (ADR-001 Decision 1). Typed deserialization replaces the bash tool's
/// `sed -> perl -> jq` scrape of reasoning-polluted JSON (FR-16, FR-19).
/// `Serialize`/`Clone` so the plan can be persisted to (and advanced in) the
/// per-repo cache (CLO-491, FR-25).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Plan {
    pub groups: Vec<Group>,
}

/// One logical commit: the files it covers, a one-line summary, and (for
/// `groups[0]` only, per the regenerate-per-group contract) a commit message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Group {
    pub files: Vec<String>,
    pub summary: String,
    /// Full conventional-commit message for `groups[0]`; `null` for later
    /// groups (we re-analyze each run, so their messages are never used here).
    pub commit_message: Option<String>,
}

/// Why a plan was rejected by [`validate_basic`]. Each maps to an announced
/// fallback to the single-commit path (FR-23 basic; full validation is CLO-492).
#[derive(Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The plan has no groups at all (`groups: []`).
    NoGroups,
    /// Group 1 references no files - nothing to commit.
    EmptyFirstGroup,
    /// Group 1 has a null/empty commit message (the exact bash null-message bug).
    MissingFirstMessage,
    /// A plan file is not in the real change set (a hallucinated path).
    UnknownFile(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::NoGroups => write!(f, "plan contained no groups"),
            PlanError::EmptyFirstGroup => write!(f, "group 1 references no files"),
            PlanError::MissingFirstMessage => {
                write!(f, "group 1 has no commit message")
            }
            PlanError::UnknownFile(p) => {
                write!(f, "group 1 references unknown file '{p}'")
            }
        }
    }
}

/// The inner JSON Schema object sent with `response_format` (ADR-001 Decision 5,
/// Groq strict mode): every property is `required` and every object sets
/// `additionalProperties: false`; `commit_message` is nullable so later groups
/// can carry `null`.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "groups": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "files": { "type": "array", "items": { "type": "string" } },
                        "summary": { "type": "string" },
                        "commit_message": { "type": ["string", "null"] }
                    },
                    "required": ["files", "summary", "commit_message"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["groups"],
        "additionalProperties": false
    })
}

/// Basic plan validation (FR-23 basic): the plan must have at least one group,
/// group 1 must be non-empty and carry a usable message, and no group may
/// reference a file absent from the real change set. Full bijective validation
/// (every changed file covered exactly once) is CLO-492.
pub fn validate_basic(plan: &Plan, change_set: &HashSet<String>) -> Result<(), PlanError> {
    let first = plan.groups.first().ok_or(PlanError::NoGroups)?;
    if first.files.is_empty() {
        return Err(PlanError::EmptyFirstGroup);
    }
    match &first.commit_message {
        Some(m) if !m.trim().is_empty() => {}
        _ => return Err(PlanError::MissingFirstMessage),
    }
    // No group may reference a file outside the real change set (catches
    // hallucinated paths). Full coverage/bijection checks are CLO-492.
    for group in &plan.groups {
        for file in &group.files {
            if !change_set.contains(file) {
                return Err(PlanError::UnknownFile(file.clone()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change_set(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    fn parse(json_str: &str) -> Plan {
        serde_json::from_str(json_str).expect("valid plan json")
    }

    #[test]
    fn deserializes_typed_plan() {
        let p = parse(
            r#"{"groups":[
                {"files":["a.rs"],"summary":"core","commit_message":"feat: a"},
                {"files":["b.md"],"summary":"docs","commit_message":null}
            ]}"#,
        );
        assert_eq!(p.groups.len(), 2);
        assert_eq!(p.groups[0].files, vec!["a.rs"]);
        assert_eq!(p.groups[0].commit_message.as_deref(), Some("feat: a"));
        assert_eq!(p.groups[1].commit_message, None);
    }

    #[test]
    fn accepts_a_valid_plan() {
        let p =
            parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"feat: a"}]}"#);
        assert_eq!(validate_basic(&p, &change_set(&["a.rs", "b.md"])), Ok(()));
    }

    #[test]
    fn rejects_empty_groups() {
        let p = parse(r#"{"groups":[]}"#);
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::NoGroups)
        );
    }

    #[test]
    fn rejects_empty_first_group() {
        let p = parse(r#"{"groups":[{"files":[],"summary":"s","commit_message":"feat: a"}]}"#);
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::EmptyFirstGroup)
        );
    }

    #[test]
    fn rejects_null_message_in_group1() {
        // The exact bash null-message bug: must be caught, not silently single-committed.
        let p = parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":null}]}"#);
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::MissingFirstMessage)
        );
    }

    #[test]
    fn rejects_blank_message_in_group1() {
        let p = parse(r#"{"groups":[{"files":["a.rs"],"summary":"s","commit_message":"   "}]}"#);
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::MissingFirstMessage)
        );
    }

    #[test]
    fn rejects_unknown_file() {
        let p = parse(
            r#"{"groups":[
                {"files":["a.rs"],"summary":"s","commit_message":"feat: a"},
                {"files":["ghost.rs"],"summary":"s2","commit_message":null}
            ]}"#,
        );
        assert_eq!(
            validate_basic(&p, &change_set(&["a.rs"])),
            Err(PlanError::UnknownFile("ghost.rs".to_string()))
        );
    }

    #[test]
    fn schema_is_strict_compatible() {
        let s = schema();
        assert_eq!(s["additionalProperties"], json!(false));
        let item = &s["properties"]["groups"]["items"];
        assert_eq!(item["additionalProperties"], json!(false));
        // strict mode requires every property to be listed in `required`.
        assert_eq!(
            item["required"],
            json!(["files", "summary", "commit_message"])
        );
        assert_eq!(
            item["properties"]["commit_message"]["type"],
            json!(["string", "null"])
        );
    }
}

 succeeded in 0ms:
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::GcmError;

/// Thin typed wrapper over the `git` binary (ADR-001 #1). All path-reading
/// commands pass `-c core.quotePath=false` and operate from the repo root so
/// porcelain/diff paths and filesystem paths agree.
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// Discover the enclosing work tree. `Ok(None)` when CWD is not inside a git
    /// repository; `Err` only when the `git` binary itself cannot be run.
    pub fn discover() -> Result<Option<Repo>, GcmError> {
        let inside = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git: {e}")))?;
        if !inside.status.success() || String::from_utf8_lossy(&inside.stdout).trim() != "true" {
            return Ok(None);
        }
        let top = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git: {e}")))?;
        if !top.status.success() {
            return Ok(None);
        }
        let root = String::from_utf8_lossy(&top.stdout).trim().to_string();
        Ok(Some(Repo {
            root: PathBuf::from(root),
        }))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A `git` Command rooted at the repo with quotePath disabled.
    fn git(&self, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        c.current_dir(&self.root);
        c.args(["-c", "core.quotePath=false"]);
        c.args(args);
        c
    }

    /// Run a git command, capturing stdout as a (lossy) UTF-8 string.
    fn capture(&self, args: &[&str]) -> Result<String, GcmError> {
        let out = self
            .git(args)
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", args.join(" "))))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Whether HEAD resolves (false on an unborn branch / fresh repo).
    pub fn has_head(&self) -> bool {
        self.git(&["rev-parse", "--verify", "--quiet", "HEAD"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// True if there are any uncommitted changes: unstaged, staged, or untracked
    /// (gitignore-respecting). Drives the "no changes -> exit 0" path (FR-9).
    pub fn has_changes(&self) -> Result<bool, GcmError> {
        let unstaged = !self.quiet_diff(&["diff", "--quiet"])?;
        let staged = !self.quiet_diff(&["diff", "--cached", "--quiet"])?;
        let untracked = !self.untracked_files()?.is_empty();
        Ok(unstaged || staged || untracked)
    }

    /// Run a `--quiet` diff; returns true when there is NO difference (exit 0).
    fn quiet_diff(&self, args: &[&str]) -> Result<bool, GcmError> {
        let status = self
            .git(args)
            .status()
            .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", args.join(" "))))?;
        Ok(status.success())
    }

    /// Diff stat for the prompt header. With HEAD, `git diff HEAD` covers all
    /// tracked changes. On an unborn branch (no HEAD) the empty-tree object may
    /// not exist in a fresh repo (so `git diff <empty-tree>` errors), thus we
    /// combine unstaged (working vs index) and staged (index vs empty) diffs -
    /// together they capture every tracked change, incl. a staged-then-modified
    /// file - and gather untracked files separately (AC-14).
    pub fn diff_stat(&self) -> Result<String, GcmError> {
        if self.has_head() {
            self.capture(&["diff", "--stat", "HEAD"])
        } else {
            let unstaged = self.capture(&["diff", "--stat"])?;
            let staged = self.capture(&["diff", "--cached", "--stat"])?;
            Ok(format!("{unstaged}{staged}"))
        }
    }

    /// Full diff (no color) for the prompt body. HEAD when present; otherwise
    /// unstaged + staged on an unborn branch. See [`Self::diff_stat`] for the
    /// unborn-branch rationale.
    pub fn diff_full(&self) -> Result<String, GcmError> {
        if self.has_head() {
            self.capture(&["diff", "--no-color", "HEAD"])
        } else {
            let unstaged = self.capture(&["diff", "--no-color"])?;
            let staged = self.capture(&["diff", "--no-color", "--cached"])?;
            Ok(format!("{unstaged}{staged}"))
        }
    }

    /// Diff `--stat` scoped to specific paths (CLO-491 per-group message header).
    /// Same HEAD/unborn handling as [`Self::diff_stat`]. Empty `paths` returns an
    /// empty string rather than an unscoped whole-tree diff.
    pub fn diff_stat_for(&self, paths: &[&str]) -> Result<String, GcmError> {
        if paths.is_empty() {
            return Ok(String::new());
        }
        if self.has_head() {
            self.capture_scoped(&["diff", "--stat", "HEAD"], paths)
        } else {
            let unstaged = self.capture_scoped(&["diff", "--stat"], paths)?;
            let staged = self.capture_scoped(&["diff", "--stat", "--cached"], paths)?;
            Ok(format!("{unstaged}{staged}"))
        }
    }

    /// Full diff (no color) scoped to specific paths (CLO-491 per-group message
    /// body). Same HEAD/unborn handling as [`Self::diff_full`]. Empty `paths`
    /// returns an empty string.
    pub fn diff_full_for(&self, paths: &[&str]) -> Result<String, GcmError> {
        if paths.is_empty() {
            return Ok(String::new());
        }
        if self.has_head() {
            self.capture_scoped(&["diff", "--no-color", "HEAD"], paths)
        } else {
            let unstaged = self.capture_scoped(&["diff", "--no-color"], paths)?;
            let staged = self.capture_scoped(&["diff", "--no-color", "--cached"], paths)?;
            Ok(format!("{unstaged}{staged}"))
        }
    }

    /// Like [`Self::capture`] but appends `-- <paths>` with
    /// `GIT_LITERAL_PATHSPECS=1`, so a filename containing a glob metacharacter
    /// (`*`, `?`) cannot pull in siblings (the CLO-487 review-2 #3 hazard).
    fn capture_scoped(&self, base: &[&str], paths: &[&str]) -> Result<String, GcmError> {
        let mut cmd = self.git(base);
        cmd.env("GIT_LITERAL_PATHSPECS", "1");
        cmd.arg("--");
        cmd.args(paths);
        let out = cmd
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", base.join(" "))))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git {} failed: {}",
                base.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Untracked files honoring gitignore (`--exclude-standard`), NUL-split so
    /// unicode/space/newline paths survive (FR-31, FR-48).
    pub fn untracked_files(&self) -> Result<Vec<String>, GcmError> {
        let out = self
            .git(&["ls-files", "--others", "--exclude-standard", "-z"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git ls-files: {e}")))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git ls-files failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(out
            .stdout
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect())
    }

    /// Capture the current index as a tree object (FR-47 transaction start).
    pub fn snapshot_index(&self) -> Result<String, GcmError> {
        Ok(self.capture(&["write-tree"])?.trim().to_string())
    }

    /// Restore the index to a previously-snapshotted tree. The working tree is
    /// untouched; this only rewinds staging (FR-47 restore on abort/failure).
    pub fn restore_index(&self, tree: &str) -> Result<(), GcmError> {
        self.capture(&["read-tree", tree]).map(|_| ())
    }

    /// Stage every change (the tracer commits all changes as one commit, FR-6).
    pub fn stage_all(&self) -> Result<(), GcmError> {
        self.capture(&["add", "-A"]).map(|_| ())
    }

    /// Create a signed commit (FR-4). Stdio is inherited so GPG/SSH passphrase
    /// (pinentry) prompts work on the user's terminal.
    ///
    /// A non-zero `git commit` (a rejecting pre-commit hook, a signing failure)
    /// returns [`GcmError::CommitFailed`], not [`GcmError::Git`]: the caller
    /// leaves the staged group in place and does not advance the plan cache
    /// (CLO-491, FR-58). A failure to even spawn `git` is a `Git` error (no
    /// commit was attempted, so the staged group should be rolled back).
    pub fn commit_signed(&self, message: &str) -> Result<(), GcmError> {
        let status = self
            .git(&["commit", "-S", "-m", message])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| GcmError::Git(format!("failed to run git commit: {e}")))?;
        if !status.success() {
            return Err(GcmError::CommitFailed(
                "git commit failed (see output above)".to_string(),
            ));
        }
        Ok(())
    }

    /// The full changed-file set for grouping, from
    /// `git status --porcelain=v1 -uall -z`. `-uall` expands untracked
    /// directories to individual files so these paths match the per-file diff
    /// paths (CLO-487 review-2 #1). NUL-delimited; renames carry their orig path.
    pub fn changed_files(&self) -> Result<Vec<ChangedFile>, GcmError> {
        let out = self
            .git(&["status", "--porcelain=v1", "-uall", "-z"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git status: {e}")))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git status failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(parse_status_z(&out.stdout))
    }

    /// True if a merge is in progress (`.git/MERGE_HEAD` exists). Combined with
    /// [`ChangedFile::is_unmerged`] this distinguishes a clean merge (commit it)
    /// from a conflicted one (abort) - CLO-487 review-2 #2.
    pub fn is_merging(&self) -> bool {
        self.git(&["rev-parse", "--verify", "--quiet", "MERGE_HEAD"])
            .output()
            .map(|o| o.status.success())

exec
/bin/zsh -lc "sed -n '1,320p' scripts/acceptance.sh" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
#!/usr/bin/env bash
# End-to-end acceptance checks for the gcm single-commit tracer (CLO-486).
#
# Most cases run offline against a mock Groq server (a tiny python responder that
# captures the request body), so they need no real GROQ_API_KEY and no network.
# Cases that create a real signed commit are gated on whether commit signing works
# in this environment. A real-network smoke test runs only when GCM_LIVE=1.
#
# Usage:  ./scripts/acceptance.sh
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${GCM_BIN:-$ROOT/target/release/gcm}"
PASS=0
FAIL=0
SKIP=0

note()  { printf '\n\033[1m== %s\033[0m\n' "$*"; }
ok()    { PASS=$((PASS+1)); printf '  \033[32mPASS\033[0m %s\n' "$*"; }
bad()   { FAIL=$((FAIL+1)); printf '  \033[31mFAIL\033[0m %s\n' "$*"; }
skip()  { SKIP=$((SKIP+1)); printf '  \033[33mSKIP\033[0m %s\n' "$*"; }

[ -x "$BIN" ] || { echo "building release binary..."; (cd "$ROOT" && cargo build --release) || exit 1; }

# --- mock Groq server -------------------------------------------------------
PORT=8731
CAPTURE="$(mktemp)"
PLAN_FILE="$(mktemp)"   # grouping tests stage a JSON plan here; empty -> fallback
MOCK_PY="$(mktemp).py"
# Redirect the plan cache (CLO-491) to a throwaway dir so the suite is hermetic
# and never pollutes the real OS cache. Scratch repos have unique paths -> unique
# cache keys, so a single shared dir is collision-free across cases.
GCM_CACHE_DIR="$(mktemp -d)"; export GCM_CACHE_DIR
cat > "$MOCK_PY" <<'PY'
import http.server, json, os, sys
CAP = os.environ["CAPTURE_FILE"]
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(n)
        with open(CAP, "ab") as f:
            f.write(body + b"\n")
        # Route by path prefix so error paths are testable (AC-12).
        if "/fail500/" in self.path:
            self.send_response(500); self.end_headers(); self.wfile.write(b"server error"); return
        is_plan = b'"response_format"' in body
        if "/empty/" in self.path:
            content = "   \n  "   # whitespace-only -> EmptyResponse
        elif is_plan:
            # Grouping (structured-output) request: return the JSON plan the
            # current test staged in PLAN_FILE. Absent/empty -> a non-JSON string
            # that forces the parse-failure fallback to single-commit.
            content = "not a json plan"
            try:
                with open(os.environ.get("PLAN_FILE", "")) as pf:
                    txt = pf.read().strip()
                    if txt:
                        content = txt
            except Exception:
                pass
        else:
            content = "chore(test): mock commit message"
        resp = json.dumps({"choices":[{"message":{"content":content}}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type","application/json")
        self.send_header("Content-Length", str(len(resp)))
        self.end_headers()
        self.wfile.write(resp)
    def log_message(self, *a): pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY

MOCK_PID=""
start_mock() {
  : > "$CAPTURE"
  CAPTURE_FILE="$CAPTURE" PLAN_FILE="$PLAN_FILE" python3 "$MOCK_PY" "$PORT" >/dev/null 2>&1 &
  MOCK_PID=$!
  for _ in $(seq 1 20); do
    if curl -s -o /dev/null "http://127.0.0.1:$PORT" 2>/dev/null; then break; fi
    sleep 0.1
  done
}
stop_mock() { [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null; MOCK_PID=""; }
cleanup() { stop_mock; rm -f "$CAPTURE" "$MOCK_PY" "$PLAN_FILE"; rm -rf "$GCM_CACHE_DIR"; }
trap cleanup EXIT

MOCK_URL="http://127.0.0.1:$PORT/openai/v1"

# --- scratch repo helper ----------------------------------------------------
new_repo() {
  d="$(mktemp -d)"
  git -C "$d" init -q
  git -C "$d" config user.email test@example.com
  git -C "$d" config user.name "Test"
  echo "$d"
}

# Does signing work here? (global config may require an SSH/GPG key + agent.)
SIGNING_OK=0
probe_signing() {
  d="$(new_repo)"
  echo x > "$d/x"
  git -C "$d" add x
  if git -C "$d" commit -S -m "probe" -q >/dev/null 2>&1; then SIGNING_OK=1; fi
  rm -rf "$d"
}
probe_signing

# ---------------------------------------------------------------------------
note "AC-5: no changes -> exit 0; non-repo -> exit 1"
d="$(new_repo)"; ( cd "$d" && "$BIN" >/tmp/gcm-out 2>&1 ); rc=$?
grep -q "No changes to commit" /tmp/gcm-out && [ $rc -eq 0 ] && ok "clean repo: exit 0 + message" || bad "clean repo (rc=$rc)"
rm -rf "$d"
nd="$(mktemp -d)"; ( cd "$nd" && "$BIN" >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -qi "not a git repository" /tmp/gcm-out && ok "non-repo: exit 1 + message" || bad "non-repo (rc=$rc)"
rm -rf "$nd"

note "AC-9: usage error -> 2; --version build-stamped"
"$BIN" --bogus >/dev/null 2>&1; [ $? -eq 2 ] && ok "bad flag -> exit 2" || bad "bad flag exit code"
"$BIN" --version | grep -Eq '^gcm [0-9]+\.[0-9]+\.[0-9]+\+[0-9a-f]+' && ok "--version has version+sha" || bad "--version format"

note "AC-8/AC-10: egress disclosure + no LLM CLI subprocess"
"$BIN" --help 2>&1 | grep -qi "sent" && ok "--help discloses egress" || bad "--help egress"
grep -qiE "egress|sends your working-tree" "$ROOT/README.md" && ok "README discloses egress" || bad "README egress"
if grep -REn 'Command::new\("(mods|crush|claude)"' "$ROOT/src" >/dev/null 2>&1; then bad "found LLM CLI subprocess"; else ok "no mods/crush/claude subprocess in src"; fi

note "AC-6: missing GROQ_API_KEY -> exit 1, index untouched"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && env -u GROQ_API_KEY GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -q "GROQ_API_KEY" /tmp/gcm-out && ok "missing key -> exit 1 + names var" || bad "missing key (rc=$rc)"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "index untouched after missing-key" || bad "index mutated"
rm -rf "$d"

note "AC-11: non-TTY without --yes -> exit non-zero (no hang)"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" </dev/null >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -ne 0 ] && grep -qi "terminal\|--yes" /tmp/gcm-out && ok "non-TTY no --yes -> exit $rc + guidance" || bad "non-TTY guard (rc=$rc)"
rm -rf "$d"

note "AC-12: unreachable provider -> exit 1, index untouched"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:9/openai/v1" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "unreachable host -> exit 1" || bad "unreachable host (rc=$rc)"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "index untouched after transport error" || bad "index mutated"
rm -rf "$d"

# Cases below talk to the mock server.
start_mock

note "AC-3: gitignored .env never sent to the provider"
d="$(new_repo)"
printf 'SECRET=topsecretvalue123\n' > "$d/.env"
printf '.env\n' > "$d/.gitignore"
printf 'real change\n' > "$d/code.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 )
if grep -q "topsecretvalue123" "$CAPTURE" || grep -q '"\.env"' "$CAPTURE" || grep -q '+++ b/.env' "$CAPTURE"; then
  bad ".env content reached the request body"
else
  ok ".env excluded from request body"
fi
rm -rf "$d"

note "AC-safe-files: untracked symlink/FIFO are name-only (no follow, no freeze)"
outside="$(mktemp -d)"; printf 'SENSITIVE_OUTSIDE_CONTENT_xyz\n' > "$outside/secret"
d="$(new_repo)"; printf 'real\n' > "$d/real.txt"
ln -s "$outside/secret" "$d/link"
mkfifo "$d/pipe" 2>/dev/null
: > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" timeout 10 "$BIN" --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ "$rc" -ne 124 ] && ok "did not hang on FIFO (rc=$rc)" || bad "hung on FIFO (timeout)"
grep -q "SENSITIVE_OUTSIDE_CONTENT_xyz" "$CAPTURE" && bad "symlink target content leaked" || ok "symlink target not followed"
grep -q "not a regular file" "$CAPTURE" && ok "special files listed name-only" || bad "no name-only marker for special files"
rm -rf "$d" "$outside"

note "AC-4: thousands of untracked files -> cap engages, no freeze"
d="$(new_repo)"; mkdir -p "$d/junk"
# 2000 files: enough to prove no-freeze and the 50-file cap, while the name-only
# listing stays under MAX_TOTAL_BYTES so the count is exact (no mid-entry cut).
# --all takes the single-commit path (one diff gather -> one request), so the
# capture counts are exact (the grouping path would gather twice: plan + fallback).
for i in $(seq 1 2000); do printf 'x' > "$d/junk/f$i.txt"; done
: > "$CAPTURE"
start=$(date +%s)
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --dry-run >/tmp/gcm-out 2>&1 )
elapsed=$(( $(date +%s) - start ))
# The captured request body is JSON (newlines escaped), so count substring
# occurrences, not lines. Every junk file appears as a "+++ b/junk/" header;
# beyond-cap files carry a "untracked cap reached" marker (name-only, no read).
total=$(grep -o '+++ b/junk/' "$CAPTURE" 2>/dev/null | wc -l | tr -d ' '); total=${total:-0}
nameonly=$(grep -o 'untracked cap reached' "$CAPTURE" 2>/dev/null | wc -l | tr -d ' '); nameonly=${nameonly:-0}
content_reads=$(( total - nameonly ))
[ "$elapsed" -le 5 ] && ok "completed in ${elapsed}s (<=5s)" || bad "too slow (${elapsed}s)"
[ "$total" -gt 100 ] && [ "$content_reads" -le 50 ] && ok "content read for <=50 of $total files ($content_reads)" || bad "cap not enforced ($content_reads reads of $total)"
[ "$nameonly" -gt 0 ] && ok "remaining files listed name-only ($nameonly omitted)" || bad "no name-only fallback"
rm -rf "$d"

note "AC-13: failing pre-commit hook -> index restored, exit 1"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo hi > "$d/a.txt"
  mkdir -p "$d/.git/hooks"
  printf '#!/bin/sh\nexit 1\n' > "$d/.git/hooks/pre-commit"; chmod +x "$d/.git/hooks/pre-commit"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 1 ] && ok "pre-commit reject -> exit 1" || bad "pre-commit reject (rc=$rc)"
  git -C "$d" diff --cached --quiet && ok "index restored after failed commit" || bad "index left staged"
  [ -z "$(git -C "$d" log --oneline 2>/dev/null)" ] && ok "no commit created" || bad "a commit slipped through"
  rm -rf "$d"
else
  skip "AC-13 needs working commit signing (not available here)"
fi

note "AC-1: dirty repo (binary + unicode name) -> one signed commit (mock message)"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  echo "code change" > "$d/main.txt"
  printf '\x00\x01\x02\x03\xff\xfe' > "$d/blob.bin"
  printf 'unicode body\n' > "$d/файл.txt"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "exit 0" || bad "commit run (rc=$rc; $(tail -1 /tmp/gcm-out))"
  n=$(git -C "$d" log --oneline 2>/dev/null | wc -l | tr -d ' ')
  [ "$n" = "1" ] && ok "exactly one commit" || bad "commit count = $n"
  git -C "$d" log -1 --pretty=%s | grep -Eq '^(feat|fix|docs|style|refactor|test|chore)(\(.+\))?!?: .+' && ok "message matches CC header" || bad "message not CC-shaped"
  # The commit carries a signature (gpgsig header) regardless of whether this env
  # can verify it (SSH verification needs an allowedSignersFile).
  git -C "$d" cat-file commit HEAD | grep -q '^gpgsig' && ok "commit is signed (gpgsig header present)" || bad "commit not signed"
  git -C "$d" -c core.quotePath=false ls-files | grep -q 'файл.txt' && ok "unicode-named file committed" || bad "unicode file missing"
  rm -rf "$d"
else
  skip "AC-1 needs working commit signing (not available here)"
fi

note "AC-14: unborn branch -> first signed commit"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo "first file" > "$d/first.txt"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "first commit on unborn branch" || bad "unborn first commit (rc=$rc)"
  rm -rf "$d"
else
  skip "AC-14 needs working commit signing (not available here)"
fi

note "AC-12b: provider HTTP 500 -> exit 1, index untouched"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/fail500/v1" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "HTTP 500 -> exit 1" || bad "HTTP 500 (rc=$rc)"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "index untouched after 500" || bad "index mutated after 500"
rm -rf "$d"

note "AC-12c: empty/whitespace provider response -> exit 1"
d="$(new_repo)"; echo hi > "$d/a.txt"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="http://127.0.0.1:$PORT/empty/v1" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -qi "empty" /tmp/gcm-out && ok "empty response -> exit 1" || bad "empty response (rc=$rc)"
rm -rf "$d"

note "AC-14b: unborn branch, staged-then-modified file -> unstaged delta captured"
d="$(new_repo)"; printf 'one\n' > "$d/s.txt"; git -C "$d" add s.txt; printf 'two\n' >> "$d/s.txt"
: > "$CAPTURE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 )
grep -q '+two' "$CAPTURE" && ok "unstaged change to staged file is in the diff" || bad "unstaged delta missing on unborn"
rm -rf "$d"

note "AC-2: abort path leaves the index unchanged (PTY)"
if command -v expect >/dev/null 2>&1 && [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; echo hi > "$d/a.txt"; git -C "$d" add a.txt; echo more >> "$d/a.txt"
  before="$(git -C "$d" write-tree)"
  GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" GCM_BIN="$BIN" GCM_DIR="$d" expect -c '
    set timeout 20
    spawn -noecho sh -c "cd $env(GCM_DIR) && $env(GCM_BIN)"
    expect {
      -re {\[Y/n/e} { send "n\r" }
      timeout { exit 3 }
    }
    expect eof
    catch wait result
    exit [lindex $result 3]
  ' >/tmp/gcm-out 2>&1; rc=$?
  after="$(git -C "$d" write-tree)"
  [ $rc -eq 0 ] && ok "abort -> exit 0" || bad "abort exit (rc=$rc)"
  [ "$before" = "$after" ] && ok "index tree unchanged after abort" || bad "index changed after abort"
  [ -z "$(git -C "$d" log --oneline 2>/dev/null)" ] && ok "no commit created on abort" || bad "commit created on abort"
  rm -rf "$d"
else
  skip "AC-2 PTY abort needs 'expect' + signing (covered structurally: staging only happens post-confirm; restore path covered by AC-13)"
fi

note "AC-7: edit path"
skip "AC-7 (\$EDITOR edit) requires interactive TTY; verify manually"

# --- CLO-487 semantic grouping ---------------------------------------------
# These stage a JSON plan in $PLAN_FILE; the mock returns it for the grouping
# (structured-output) request. Setup commits disable signing so they run even
# where signing is unavailable; the gcm commit itself still uses `-S`.

note "AC-G1: mixed change set splits; group 1 commits, the rest stays dirty"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: update src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "exit 0" || bad "group commit (rc=$rc; $(tail -1 /tmp/gcm-out))"
  [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "2" ] && ok "one new commit (group 1)" || bad "wrong commit count"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'src.txt' && ok "group 1 file committed" || bad "src.txt not committed"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'docs.md' && bad "docs.md leaked into group 1" || ok "group 2 file excluded from commit"
  git -C "$d" status --porcelain | grep -q 'docs.md' && ok "group 2 file left dirty for next run" || bad "docs.md not left dirty"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G1 needs signing"
fi

note "AC-G2: re-run commits the next group (progression without a cache)"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )

exec
/bin/zsh -lc "sed -n '261,520p' src/groq.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
exec
/bin/zsh -lc "sed -n '261,520p' src/git.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
exec
/bin/zsh -lc "sed -n '281,520p' src/diff.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
        assert_eq!(strip_think(s).trim(), "fix: x");
    }

    #[test]
    fn drops_unterminated_think() {
        let s = "docs: update\n<think>oops never closed";
        assert_eq!(strip_think(s).trim(), "docs: update");
    }

    #[test]
    fn leaves_clean_message_untouched() {
        let s = "chore: bump deps";
        assert_eq!(strip_think(s), "chore: bump deps");
    }

    #[test]
    fn gpt_oss_gets_include_reasoning_false() {
        let mut p = json!({ "model": "openai/gpt-oss-120b" });
        apply_reasoning_suppression(&mut p, "openai/gpt-oss-120b");
        assert_eq!(p["include_reasoning"], json!(false));
        assert!(p.get("reasoning_effort").is_none());
    }

    #[test]
    fn qwen_gets_reasoning_effort_none() {
        let mut p = json!({ "model": "qwen/qwen3.6-27b" });
        apply_reasoning_suppression(&mut p, "qwen/qwen3.6-27b");
        assert_eq!(p["reasoning_effort"], json!("none"));
        assert!(p.get("include_reasoning").is_none());
    }

    #[test]
    fn plan_payload_requests_strict_structured_output() {
        let ctx = GroupingContext {
            file_list: "a.rs\nb.md".to_string(),
            status: " M a.rs\n?? b.md".to_string(),
            stat: "2 files changed".to_string(),
            body: "diff --git a/a.rs b/a.rs".to_string(),
        };
        let p = build_plan_payload(&ctx, "openai/gpt-oss-120b");

        let rf = &p["response_format"];
        assert_eq!(rf["type"], json!("json_schema"));
        assert_eq!(rf["json_schema"]["name"], json!("commit_plan"));
        assert_eq!(rf["json_schema"]["strict"], json!(true));
        // the embedded schema is the typed Plan contract
        assert!(rf["json_schema"]["schema"]["properties"]["groups"].is_object());
        // reasoning suppression carries over for gpt-oss
        assert_eq!(p["include_reasoning"], json!(false));
        // the user message carries the grouping inputs
        let user = p["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("Changed files"));
        assert!(user.contains("a.rs"));
        assert!(user.contains("Git status"));
        assert!(user.contains("diff --git"));
    }
}

 succeeded in 0ms:
            .unwrap_or(false)
    }

    /// Reset the index to the committed state so a subsequent path-scoped
    /// `add` produces a commit of exactly those paths: `read-tree HEAD` when
    /// HEAD resolves, `read-tree --empty` on an unborn branch (no HEAD - plain
    /// `read-tree HEAD` would fail). Clearing to HEAD (not emptying) keeps
    /// other tracked files at their HEAD version so they are not recorded as
    /// deletions (CLO-487 review-1 #2).
    pub fn clear_staged(&self) -> Result<(), GcmError> {
        if self.has_head() {
            self.capture(&["read-tree", "HEAD"]).map(|_| ())
        } else {
            self.capture(&["read-tree", "--empty"]).map(|_| ())
        }
    }

    /// Stage exactly the given files (a commit group). Paths are fed
    /// NUL-separated on stdin via `--pathspec-from-file=- --pathspec-file-nul`
    /// (no `ARG_MAX` limit, no arg quoting) and `GIT_LITERAL_PATHSPECS=1`
    /// disables git's internal pathspec globbing so a filename containing `*`
    /// or `?` cannot pull in siblings (CLO-487 review-2 #3 + #4). Rename/copy
    /// entries contribute both their new and original path so the commit
    /// completes the rename (review-1 #1).
    pub fn stage_group(&self, files: &[&ChangedFile]) -> Result<(), GcmError> {
        let mut stdin_bytes: Vec<u8> = Vec::new();
        for cf in files {
            for p in cf.stage_paths() {
                stdin_bytes.extend_from_slice(p.as_bytes());
                stdin_bytes.push(0);
            }
        }
        let mut child = self
            .git(&["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"])
            .env("GIT_LITERAL_PATHSPECS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| GcmError::Git(format!("failed to run git add: {e}")))?;
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(&stdin_bytes)
            .map_err(|e| GcmError::Git(format!("failed to write pathspecs to git add: {e}")))?;
        let out = child
            .wait_with_output()
            .map_err(|e| GcmError::Git(format!("failed to run git add: {e}")))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(())
    }
}

/// One entry from `git status --porcelain=v1 -z`: the two status chars (`x`
/// staged-side, `y` worktree-side), the path (the *new* path for renames), and
/// the original path for rename/copy entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub x: u8,
    pub y: u8,
    pub path: String,
    pub orig_path: Option<String>,
}

impl ChangedFile {
    /// An unmerged (conflicted) entry - any `U`, or `DD`/`AA` (the seven
    /// unmerged XY combinations). gcm must abort rather than commit these.
    pub fn is_unmerged(&self) -> bool {
        self.x == b'U'
            || self.y == b'U'
            || (self.x == b'D' && self.y == b'D')
            || (self.x == b'A' && self.y == b'A')
    }

    /// The paths to stage for this entry: the new path, plus the original path
    /// for a rename/copy so the deletion of the old name is staged too.
    pub fn stage_paths(&self) -> Vec<&str> {
        let mut v = vec![self.path.as_str()];
        if let Some(o) = &self.orig_path {
            v.push(o.as_str());
        }
        v
    }
}

/// Parse `git status --porcelain=v1 -z` output. Each NUL-delimited record is
/// `XY<space>PATH`; for a rename/copy (`R`/`C` in X or Y) the *next* record is
/// the original path (verified empirically against real `git mv` output - the
/// new path is in the XY record, the orig path follows). NUL-delimited so a
/// path containing a space, newline, or the literal ` -> ` survives intact.
pub fn parse_status_z(bytes: &[u8]) -> Vec<ChangedFile> {
    let mut out = Vec::new();
    let mut records = bytes.split(|&b| b == 0).filter(|r| !r.is_empty());
    while let Some(rec) = records.next() {
        if rec.len() < 3 {
            continue; // malformed/short record - skip defensively
        }
        let x = rec[0];
        let y = rec[1];
        // rec[2] is the separator space; the path is everything after it.
        let path = String::from_utf8_lossy(&rec[3..]).into_owned();
        let orig_path = if x == b'R' || x == b'C' || y == b'R' || y == b'C' {
            records
                .next()
                .map(|o| String::from_utf8_lossy(o).into_owned())
        } else {
            None
        };
        out.push(ChangedFile {
            x,
            y,
            path,
            orig_path,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rename_new_path_with_orig_following() {
        // Real porcelain -z for `git mv d/orig.txt d/renamed.txt`: the XY
        // record carries the NEW path, the following record is the ORIG path.
        let raw = b"R  d/renamed.txt\0d/orig.txt\0 M mod.txt\0";
        let files = parse_status_z(raw);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "d/renamed.txt");
        assert_eq!(files[0].orig_path.as_deref(), Some("d/orig.txt"));
        assert_eq!(files[0].x, b'R');
        assert_eq!(files[1].path, "mod.txt");
        assert_eq!(files[1].orig_path, None);
    }

    #[test]
    fn arrow_in_filename_survives_nul_parse() {
        // A file literally named "a -> b.txt"; splitting on " -> " would corrupt
        // it, NUL-delimited parsing keeps it whole.
        let files = parse_status_z(b"?? a -> b.txt\0");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a -> b.txt");
        assert_eq!(files[0].orig_path, None);
    }

    #[test]
    fn deletion_and_untracked_parse() {
        let files = parse_status_z(b"D  del.txt\0?? new.txt\0");
        assert_eq!(files[0].path, "del.txt");
        assert_eq!(files[0].x, b'D');
        assert_eq!(files[1].path, "new.txt");
        assert_eq!(files[1].x, b'?');
    }

    #[test]
    fn detects_unmerged_entries() {
        assert!(parse_status_z(b"UU conflict.txt\0")[0].is_unmerged());
        assert!(parse_status_z(b"AA both-added.txt\0")[0].is_unmerged());
        assert!(!parse_status_z(b" M ok.txt\0")[0].is_unmerged());
        assert!(!parse_status_z(b"?? new.txt\0")[0].is_unmerged());
    }

    #[test]
    fn stage_paths_includes_orig_for_rename() {
        let files = parse_status_z(b"R  new.txt\0old.txt\0");
        assert_eq!(files[0].stage_paths(), vec!["new.txt", "old.txt"]);
    }

    #[test]
    fn stage_paths_single_for_non_rename() {
        let files = parse_status_z(b" M mod.txt\0");
        assert_eq!(files[0].stage_paths(), vec!["mod.txt"]);
    }

    // --- integration tests against real git -------------------------------

    fn run_git(root: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .expect("run git")
    }

    fn temp_repo() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        run_git(&root, &["init", "-q"]);
        run_git(&root, &["config", "user.email", "t@t"]);
        run_git(&root, &["config", "user.name", "T"]);
        let repo = Repo { root };
        (dir, repo)
    }

    fn staged_names(root: &Path) -> Vec<String> {
        let out = run_git(root, &["diff", "--cached", "--name-only"]);
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect()
    }

    fn cf_for<'a>(files: &'a [ChangedFile], path: &str) -> &'a ChangedFile {
        files
            .iter()
            .find(|c| c.path == path)
            .expect("path in change set")
    }

    #[test]
    fn stage_group_isolates_literal_glob_filename() {
        // A file literally named `a*.txt` must stage ONLY itself, never glob
        // siblings like `ab.txt` (GIT_LITERAL_PATHSPECS=1).
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("a*.txt"), "1").unwrap();
        std::fs::write(root.join("ab.txt"), "1").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "init"]);
        std::fs::write(root.join("a*.txt"), "2").unwrap();
        std::fs::write(root.join("ab.txt"), "2").unwrap();

        let files = repo.changed_files().unwrap();
        repo.clear_staged().unwrap();
        repo.stage_group(&[cf_for(&files, "a*.txt")]).unwrap();

        assert_eq!(staged_names(root), vec!["a*.txt".to_string()]);
    }

    #[test]
    fn stage_group_completes_a_rename() {
        // Staging a rename must stage BOTH the new path and the deletion of the
        // original, so the index reflects a completed rename (not a stray copy).
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("old.txt"), "content").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "init"]);
        run_git(root, &["mv", "old.txt", "new.txt"]); // stages the rename (R)

        let files = repo.changed_files().unwrap();
        let rename = cf_for(&files, "new.txt");
        assert_eq!(rename.orig_path.as_deref(), Some("old.txt"));
        repo.clear_staged().unwrap();
        repo.stage_group(&[rename]).unwrap();

        // The index now tracks new.txt and no longer tracks old.txt.
        let ls = run_git(root, &["ls-files"]);
        let tracked = String::from_utf8_lossy(&ls.stdout);
        assert!(tracked.contains("new.txt"), "new path staged");
        assert!(!tracked.contains("old.txt"), "old path deletion staged");
    }


 succeeded in 0ms:
        // A trailing multibyte char split by the 8192-byte window is fine (<=3 bytes).
        Err(e) => e.valid_up_to() < sample.len().saturating_sub(3),
    }
}

/// Per-file binary elision for a tracked diff (port of git-commit-ai.sh:87-119):
/// if a file's hunk body is mostly non-text, keep the `diff --git` header and
/// replace the body with a placeholder; otherwise strip stray NULs and keep it.
fn elide_binary_diff(diff: &str) -> String {
    let mut out = String::new();
    let mut buf = String::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") {
            flush_section(&buf, &mut out);
            buf.clear();
        }
        buf.push_str(line);
    }
    flush_section(&buf, &mut out);
    out
}

fn flush_section(section: &str, out: &mut String) {
    if section.is_empty() {
        return;
    }
    let mut header = String::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in section.split_inclusive('\n') {
        if !in_body && line.starts_with("@@") {
            in_body = true;
        }
        if in_body {
            body.push_str(line);
        } else {
            header.push_str(line);
        }
    }

    let mut sample = String::new();
    for line in body.lines() {
        let stripped = line
            .strip_prefix(|c| c == '+' || c == '-' || c == ' ')
            .unwrap_or(line);
        sample.push_str(stripped);
    }
    // Count NUL, the UTF-8 replacement char (U+FFFD, what lossy decoding turns
    // raw binary bytes into), and control chars as "non-text". Valid non-ASCII
    // text (Cyrillic, CJK, etc.) is NOT counted, so it is never wrongly elided.
    let total = sample.chars().count();
    let non_text = sample
        .chars()
        .filter(|&c| {
            c == '\u{0}'
                || c == '\u{FFFD}'
                || (c.is_control() && c != '\t' && c != '\n' && c != '\r')
        })
        .count();

    if total > 200 && (non_text as f64) / (total as f64) > 0.10 {
        out.push_str(&header);
        if !header.ends_with('\n') {
            out.push('\n');
        }
        let lines = body.lines().count();
        out.push_str(&format!(
            "Binary files differ (body elided: {total} bytes, {lines} diff lines)\n"
        ));
    } else {
        out.push_str(&section.replace('\0', ""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_text_is_not_binary() {
        assert!(!looks_binary(b"fn main() {}\n"));
    }

    #[test]
    fn utf8_unicode_text_is_not_binary() {
        assert!(!looks_binary("файл: привет мир\n".as_bytes()));
    }

    #[test]
    fn nul_bytes_are_binary() {
        // A file git's 8000-byte heuristic could misclassify as text but which
        // carries NUL bytes must be treated as binary (Novel #9).
        let mut data = b"looks like text for a while ".repeat(4);
        data.push(0);
        data.extend_from_slice(b"more");
        assert!(looks_binary(&data));
    }

    #[test]
    fn invalid_utf8_is_binary() {
        assert!(looks_binary(&[0xff, 0xfe, 0xfd, 0x00, 0x01, 0x02]));
    }

    #[test]
    fn text_diff_is_preserved() {
        let diff = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-old\n+new\n";
        let out = elide_binary_diff(diff);
        assert!(out.contains("+new"));
        assert!(!out.contains("body elided"));
    }

    #[test]
    fn cyrillic_text_diff_is_not_elided() {
        // Valid non-ASCII (UTF-8) text must not be misclassified as binary even
        // though every Cyrillic byte is > 0x7e.
        let mut diff = String::from("diff --git a/doc.txt b/doc.txt\n@@ -0,0 +1 @@\n");
        for _ in 0..50 {
            diff.push_str("+Добавлен новый раздел документации про настройку\n");
        }
        let out = elide_binary_diff(&diff);
        assert!(out.contains("Добавлен"), "Cyrillic text preserved");
        assert!(!out.contains("body elided"), "valid UTF-8 not elided");
    }

    #[test]
    fn binary_diff_body_is_elided() {
        let mut diff = String::from("diff --git a/img.png b/img.png\n@@ -0,0 +1 @@\n");
        // A long, mostly-non-text body.
        for _ in 0..50 {
            diff.push('+');
            diff.push_str("\u{0}\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\u{e}\n");
        }
        let out = elide_binary_diff(&diff);
        assert!(out.contains("diff --git a/img.png b/img.png"));
        assert!(out.contains("body elided"));
        assert!(!out.contains('\u{0}'));
    }

    #[test]
    fn read_capped_bounds_large_files() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(&vec![b'a'; 100_000]).unwrap();
        f.flush().unwrap();
        let (buf, more) = read_capped(f.path(), 8192).unwrap();
        assert_eq!(
            buf.len(),
            8192,
            "read is bounded to the cap, not the file size"
        );
        assert!(more, "more flag set when the file exceeds the cap");
    }

    #[test]
    fn read_capped_small_file_has_no_more() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"short").unwrap();
        f.flush().unwrap();
        let (buf, more) = read_capped(f.path(), 8192).unwrap();
        assert_eq!(buf, b"short");
        assert!(!more);
    }

    #[test]
    fn small_diff_section_is_unchanged() {
        let diff = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-old\n+new\n";
        assert_eq!(truncate_per_file(diff, 8192), diff);
    }

    #[test]
    fn large_diff_section_keeps_header_and_omits_body() {
        let mut diff =
            String::from("diff --git a/big.rs b/big.rs\n--- a/big.rs\n+++ b/big.rs\n@@ -1 +1 @@\n");
        for _ in 0..500 {
            diff.push_str("+a line of content that makes this section big\n");
        }
        let out = truncate_per_file(&diff, 200);
        assert!(
            out.contains("diff --git a/big.rs b/big.rs"),
            "diff header kept"
        );
        assert!(out.contains("+++ b/big.rs"), "file header kept");
        assert!(out.contains("[diff omitted:"), "placeholder present");
        assert!(!out.contains("a line of content"), "huge body dropped");
        assert!(out.len() < 300, "section is now small");
    }

    #[test]
    fn file_list_json_keeps_a_newline_path_as_one_element() {
        // A path containing a newline must stay a single discrete element, or
        // the model would see (and group) phantom split paths (Codex finding).
        let changed = vec![
            ChangedFile {
                x: b' ',
                y: b'M',
                path: "weird\nname.txt".to_string(),
                orig_path: None,
            },
            ChangedFile {
                x: b'?',
                y: b'?',
                path: "normal.rs".to_string(),
                orig_path: None,
            },
        ];
        let parsed: Vec<String> = serde_json::from_str(&file_list_json(&changed)).unwrap();
        assert_eq!(
            parsed,
            vec!["weird\nname.txt".to_string(), "normal.rs".to_string()]
        );
        let rows: Vec<String> = serde_json::from_str(&status_json(&changed)).unwrap();
        assert_eq!(rows[0], " M weird\nname.txt");
    }

    #[test]
    fn truncates_per_file_so_a_small_file_after_a_huge_one_survives() {
        // Whole-body tail-chop would sever the trailing small file; per-file
        // truncation keeps it intact (the CLO-487 fix).
        let mut diff = String::from("diff --git a/big.rs b/big.rs\n@@ -1 +1 @@\n");
        for _ in 0..500 {
            diff.push_str("+filler filler filler filler filler\n");
        }
        diff.push_str(
            "diff --git a/small.rs b/small.rs\n--- a/small.rs\n+++ b/small.rs\n@@ -1 +1 @@\n+tiny\n",
        );
        let out = truncate_per_file(&diff, 200);
        assert!(out.contains("diff --git a/small.rs"), "small file present");
        assert!(out.contains("+tiny"), "small file body intact");
        assert!(out.contains("[diff omitted:"), "big file elided");
        assert!(!out.contains("filler filler"), "big file body dropped");
    }
}

exec
/bin/zsh -lc "sed -n '321,640p' scripts/acceptance.sh" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: update docs"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "3" ] && ok "two grouped commits total" || bad "progression commit count"
  [ -z "$(git -C "$d" status --porcelain)" ] && ok "tree clean after both groups" || bad "tree still dirty"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G2 needs signing"
fi

note "AC-G4: rename + delete + ->-in-name + unicode group without fallback"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf 'old\n' > "$d/orig.txt"; printf 'gone\n' > "$d/del.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  git -C "$d" mv orig.txt renamed.txt
  git -C "$d" rm -q del.txt
  printf 'arrow\n' > "$d/a -> b.txt"
  printf 'uni\n' > "$d/файл.txt"
  printf '%s' '{"groups":[{"files":["renamed.txt","del.txt","a -> b.txt","файл.txt"],"summary":"mixed","commit_message":"chore: reshuffle files"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "exit 0" || bad "tricky-name group (rc=$rc; $(tail -1 /tmp/gcm-out))"
  grep -qi "Falling back" /tmp/gcm-out && bad "tripped into single-commit fallback" || ok "no fallback (grouping path held)"
  git -C "$d" -c core.quotePath=false ls-files | grep -q 'renamed.txt' && ok "rename new path committed" || bad "rename new path missing"
  git -C "$d" -c core.quotePath=false ls-files | grep -q 'orig.txt' && bad "rename old path still tracked" || ok "rename old path deleted (rename completed)"
  git -C "$d" -c core.quotePath=false ls-files | grep -qF 'a -> b.txt' && ok "arrow-in-name file committed" || bad "arrow-name file missing"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G4 needs signing"
fi

note "AC-G13: a filename containing * stages only the literal file (no glob leak)"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf '1\n' > "$d/ab.txt"; star="$d/a*.txt"; printf '1\n' > "$star"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf '2\n' > "$d/ab.txt"; printf '2\n' > "$star"
  printf '%s' '{"groups":[{"files":["a*.txt"],"summary":"star","commit_message":"feat: star file"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "exit 0" || bad "glob-name group (rc=$rc)"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'ab.txt' && bad "glob leaked ab.txt into the commit" || ok "only the literal a*.txt staged"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G13 needs signing"
fi

note "AC-G6: plan referencing an unknown file -> announced fallback"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'v1\n' > "$d/real.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/real.txt"
  printf '%s' '{"groups":[{"files":["ghost.txt"],"summary":"phantom","commit_message":"feat: ghost"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  grep -qi "Falling back" /tmp/gcm-out && ok "unknown file -> fallback announced" || bad "no fallback on unknown file"
  grep -qi "unknown file" /tmp/gcm-out && ok "reason names the unknown file" || bad "reason missing"
  [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "2" ] && ok "fallback made a single commit" || bad "fallback commit count"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G6 needs signing"
fi

note "AC-G7: unparseable plan JSON -> fallback to single-commit"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'v2\n' > "$d/real.txt"
  printf '%s' '{ this is not valid json' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  grep -qi "Falling back" /tmp/gcm-out && ok "malformed plan -> fallback" || bad "no fallback on malformed plan"
  [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "fallback single commit created" || bad "fallback commit (rc=$rc)"
  : > "$PLAN_FILE"; rm -rf "$d"
else
  skip "AC-G7 needs signing"
fi

note "AC-G8: --dry-run previews the plan and commits nothing"
d="$(new_repo)"
printf 'v1\n' > "$d/x.txt"; printf 'v1\n' > "$d/y.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm init
printf 'v2\n' > "$d/x.txt"; printf 'v2\n' > "$d/y.txt"
before="$(git -C "$d" status --porcelain | sort)"
printf '%s' '{"groups":[{"files":["x.txt"],"summary":"x change","commit_message":"feat: x"},{"files":["y.txt"],"summary":"y change","commit_message":null}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && ok "dry-run exit 0" || bad "dry-run (rc=$rc)"
grep -q "Found 2 group" /tmp/gcm-out && ok "plan groups displayed" || bad "groups not displayed"
grep -q "committing now" /tmp/gcm-out && ok "group 1 marked committing now" || bad "group 1 marker missing"
[ "$before" = "$(git -C "$d" status --porcelain | sort)" ] && ok "working tree unchanged" || bad "dry-run mutated the tree"
[ -z "$(git -C "$d" diff --cached --name-only)" ] && ok "nothing staged" || bad "dry-run staged something"
[ -z "$(git -C "$d" log --oneline 2>/dev/null | sed -n 2p)" ] && ok "no new commit" || bad "dry-run committed"
: > "$PLAN_FILE"; rm -rf "$d"

note "AC-G9: --all bypasses grouping (single commit, no plan request)"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"; printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
  : > "$CAPTURE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "--all -> one commit" || bad "--all commit (rc=$rc)"
  grep -q 'response_format' "$CAPTURE" && bad "--all still issued a grouping request" || ok "--all skipped the grouping request"
  rm -rf "$d"
else
  skip "AC-G9 needs signing"
fi

note "AC-G12: unresolved merge conflict -> abort, merge state intact"
d="$(new_repo)"
printf 'base\n' > "$d/f.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm base
main_b="$(git -C "$d" branch --show-current)"
git -C "$d" switch -q -c feature
printf 'feature\n' > "$d/f.txt"; git -C "$d" -c commit.gpgsign=false commit -qam feat
git -C "$d" switch -q "$main_b"
printf 'mainline\n' > "$d/f.txt"; git -C "$d" -c commit.gpgsign=false commit -qam mainline
git -C "$d" merge feature >/dev/null 2>&1 || true
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && ok "conflict -> exit 1" || bad "conflict exit (rc=$rc)"
grep -qi "conflict" /tmp/gcm-out && ok "message names the merge conflict" || bad "no conflict message"
git -C "$d" rev-parse --verify --quiet MERGE_HEAD >/dev/null && ok "merge still in progress (gcm did not commit)" || bad "merge state lost"
# --all must NOT bypass the conflict guard (the guard runs before --all).
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 1 ] && grep -qi "conflict" /tmp/gcm-out && ok "--all also aborts on a conflict (no marker baking)" || bad "--all bypassed the conflict guard (rc=$rc)"
git -C "$d" rev-parse --verify --quiet MERGE_HEAD >/dev/null && ok "--all left the merge in progress" || bad "--all committed during a conflict"
rm -rf "$d"

note "AC-G12c: clean merge-in-progress (MERGE_HEAD, no conflict) -> single merge commit"
if [ "$SIGNING_OK" -eq 1 ]; then
  d="$(new_repo)"
  printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm base
  main_b="$(git -C "$d" branch --show-current)"
  git -C "$d" switch -q -c feature
  printf 'a2\n' > "$d/a.txt"; git -C "$d" -c commit.gpgsign=false commit -qam feat
  git -C "$d" switch -q "$main_b"
  printf 'b2\n' > "$d/b.txt"; git -C "$d" -c commit.gpgsign=false commit -qam mainline
  git -C "$d" merge --no-commit --no-ff feature >/dev/null 2>&1 || true   # clean, staged, MERGE_HEAD set
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "clean merge -> exit 0" || bad "clean merge (rc=$rc; $(tail -1 /tmp/gcm-out))"
  git -C "$d" rev-parse --verify --quiet MERGE_HEAD >/dev/null && bad "merge not finalized" || ok "merge finalized (MERGE_HEAD cleared)"
  parents=$(git -C "$d" show -s --format=%P HEAD | wc -w | tr -d ' ')
  [ "$parents" = "2" ] && ok "HEAD is a two-parent merge commit" || bad "merge commit has $parents parents"
  rm -rf "$d"
else
  skip "AC-G12c needs signing"
fi

note "AC-uall: untracked directory expands to individual files (path agreement)"
d="$(new_repo)"
printf 'init\n' > "$d/seed.txt"
git -C "$d" -c commit.gpgsign=false add -A >/dev/null
git -C "$d" -c commit.gpgsign=false commit -qm init
mkdir "$d/pkg"; printf '1\n' > "$d/pkg/a.txt"; printf '2\n' > "$d/pkg/b.txt"
printf '%s' '{"groups":[{"files":["pkg/a.txt","pkg/b.txt"],"summary":"pkg","commit_message":"feat: pkg"}]}' > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --dry-run >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -eq 0 ] && ok "dry-run exit 0" || bad "uall dry-run (rc=$rc)"
grep -qi "Falling back" /tmp/gcm-out && bad "fallback: status collapsed pkg/ (no -uall expansion)" || ok "individual files matched plan (-uall agreement)"
grep -q "Found 1 group" /tmp/gcm-out && ok "grouping ran on the expanded files" || bad "grouping did not run"
: > "$PLAN_FILE"; rm -rf "$d"

# --- CLO-491 per-repo plan cache -------------------------------------------
# The cache lives under $GCM_CACHE_DIR (exported above). reset_cache wipes it so
# cache_file can glob the single plan file the current case produced.
reset_cache() { rm -f "$GCM_CACHE_DIR"/plan-*.json; }
cache_file()  { ls "$GCM_CACHE_DIR"/plan-*.json 2>/dev/null | head -1; }

# Stage a 2-group change set (src.txt -> group 1, docs.md -> group 2) on top of
# an initial commit. Echoes the repo dir.
cache_repo_2group() {
  local d; d="$(new_repo)"
  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
  echo "$d"
}

note "AC-C1: re-run commits group 2 from cache with no grouping call (AC-1, FR-2)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  # Run 2 is a cache hit: capture only this run, and blank the plan so any
  # (unexpected) grouping call would be visible as a fallback.
  : > "$CAPTURE"; : > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "re-run exit 0" || bad "re-run (rc=$rc; $(tail -1 /tmp/gcm-out))"
  grep -q '"response_format"' "$CAPTURE" && bad "re-run made a grouping call (cache missed)" || ok "no grouping call on re-run (cache hit)"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'docs.md' && ok "group 2 committed from cache" || bad "group 2 not committed"
  git -C "$d" log -1 --pretty=%s | grep -qi "mock commit message" && ok "group 2 carried a valid (regenerated) message" || bad "group 2 message missing"
  [ -z "$(git -C "$d" status --porcelain)" ] && ok "tree clean after group 2" || bad "tree still dirty"
  reset_cache; rm -rf "$d"
else
  skip "AC-C1 needs signing"
fi

note "AC-C2: editing a pending file invalidates the cache and re-analyzes (AC-2, FR-27)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  printf 'v3-edited\n' > "$d/docs.md"   # edit the still-pending group-2 file
  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: edited"}]}' > "$PLAN_FILE"
  : > "$CAPTURE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "re-run after edit exit 0" || bad "edit re-run (rc=$rc)"
  grep -q '"response_format"' "$CAPTURE" && ok "edit invalidated the cache -> grouping call" || bad "stale cache reused after a content edit"
  reset_cache; rm -rf "$d"
else
  skip "AC-C2 needs signing"
fi

note "AC-C3: rejecting pre-commit hook leaves the group staged + plan un-advanced (AC-3, FR-58)"
reset_cache; d="$(cache_repo_2group)"
mkdir -p "$d/.git/hooks"
printf '#!/bin/sh\nexit 1\n' > "$d/.git/hooks/pre-commit"; chmod +x "$d/.git/hooks/pre-commit"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
[ $rc -ne 0 ] && ok "rejecting hook -> exit $rc" || bad "expected non-zero on hook rejection"
grep -qi "left staged" /tmp/gcm-out && ok "error explains the group is left staged" || bad "FR-58 message missing"
git -C "$d" diff --cached --name-only | grep -qx 'src.txt' && ok "group 1 left staged for retry" || bad "group 1 not staged after hook reject"
cf="$(cache_file)"
[ -n "$cf" ] && grep -q '"src.txt"' "$cf" && ok "cache un-advanced (still holds group 1)" || bad "cache advanced despite the commit failure"
[ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "no commit created" || bad "a commit slipped through the rejecting hook"
# Removing the hook and re-running retries the same group from the cache.
rm -f "$d/.git/hooks/pre-commit"; : > "$CAPTURE"; : > "$PLAN_FILE"
( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
if [ "$SIGNING_OK" -eq 1 ]; then
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'src.txt' && ok "retry committed the same group 1 from cache" || bad "retry did not commit group 1"
else
  skip "AC-C3 retry-commit assertion needs signing"
fi
reset_cache; rm -rf "$d"

note "AC-C4: first commit in an unborn repo (no HEAD) works with the cache (AC-4)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"   # fresh repo, no commits -> unborn HEAD
  printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"},{"files":["b.txt"],"summary":"b","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
  [ $rc -eq 0 ] && ok "unborn first commit exit 0" || bad "unborn run (rc=$rc; $(tail -1 /tmp/gcm-out))"
  git -C "$d" rev-parse HEAD >/dev/null 2>&1 && ok "HEAD now exists (first commit created)" || bad "no HEAD after run"
  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'a.txt' && ok "group 1 (a.txt) committed" || bad "a.txt not committed"
  [ -n "$(cache_file)" ] && ok "cache advanced to group 2" || bad "no cache after unborn first commit"
  reset_cache; rm -rf "$d"
else
  skip "AC-C4 needs signing"
fi

note "AC-C5: cache file lives in the cache dir, named plan-<key>.json, mode 0600 (AC-5, FR-29)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  cf="$(cache_file)"
  [ -n "$cf" ] && [ -f "$cf" ] && ok "cache file created under the configured cache dir" || bad "no cache file produced"
  case "$cf" in "$GCM_CACHE_DIR"/plan-*.json) ok "name is plan-<key>.json under GCM_CACHE_DIR" ;; *) bad "unexpected cache path: $cf" ;; esac
  mode="$(stat -f '%Lp' "$cf" 2>/dev/null || stat -c '%a' "$cf" 2>/dev/null)"
  [ "$mode" = "600" ] && ok "cache file mode is 0600" || bad "cache file mode is '$mode' (want 600)"
  reset_cache; rm -rf "$d"
else
  skip "AC-C5 needs signing"
fi

note "AC-C6: --reset re-analyzes; --all clears the cache (AC-6, FR-8/FR-28)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ -n "$(cache_file)" ] && ok "cache warmed (group 2 cached)" || bad "no cache after run 1"
  : > "$CAPTURE"
  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: d"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --reset --yes >/tmp/gcm-out 2>&1 )
  grep -q '"response_format"' "$CAPTURE" && ok "--reset forced a grouping call" || bad "--reset did not re-analyze"
  reset_cache; rm -rf "$d"

  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ -n "$(cache_file)" ] && ok "cache warmed before --all" || bad "no cache to clear"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 )
  [ -z "$(cache_file)" ] && ok "--all cleared the cache" || bad "--all left the cache in place"
  reset_cache; rm -rf "$d"
else
  skip "AC-C6 needs signing"
fi

note "AC-C7: aborting at the prompt leaves the cache un-advanced (AC-7)"
if [ "$SIGNING_OK" -eq 1 ] && command -v expect >/dev/null 2>&1; then
  reset_cache; d="$(cache_repo_2group)"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  before="$(cat "$(cache_file)")"
  GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" GCM_BIN="$BIN" GCM_DIR="$d" expect -c '
    set timeout 20
    spawn -noecho sh -c "cd $env(GCM_DIR) && GROQ_API_KEY=$env(GROQ_API_KEY) GCM_GROQ_BASE_URL=$env(GCM_GROQ_BASE_URL) $env(GCM_BIN)"
    expect {
      -re {\[Y/n/e} { send "n\r" }
      timeout { exit 3 }
    }
    expect eof
  ' >/tmp/gcm-out 2>&1
  after="$(cat "$(cache_file)")"
  [ "$before" = "$after" ] && ok "cache byte-identical after abort (not advanced)" || bad "abort changed/advanced the cache"
  git -C "$d" status --porcelain | grep -q 'docs.md' && ok "group 2 still pending after abort" || bad "group 2 not pending after abort"
  reset_cache; rm -rf "$d"
else
  skip "AC-C7 needs signing + expect"
fi

note "AC-C11: a single-group plan deletes the cache after its commit (eval 11)"
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"
  printf 'v1\n' > "$d/only.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  printf 'v2\n' > "$d/only.txt"
  printf '%s' '{"groups":[{"files":["only.txt"],"summary":"only","commit_message":"feat: only"}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  [ -z "$(cache_file)" ] && ok "single-group plan left no cache (nothing to advance to)" || bad "cache lingered after the last group"
  reset_cache; rm -rf "$d"
else
  skip "AC-C11 needs signing"
fi

note "AC-C21: a group's message excludes other groups' untracked files (blind-spot #3, eval 21)"

2026-06-20T19:31:48.533535Z ERROR codex_core::tools::router: error=failed to parse function arguments: duplicate field `workdir` at line 1 column 104
exec
/bin/zsh -lc "sed -n '641,860p' scripts/acceptance.sh" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
if [ "$SIGNING_OK" -eq 1 ]; then
  reset_cache; d="$(new_repo)"
  printf 'seed\n' > "$d/seed.txt"
  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
  git -C "$d" -c commit.gpgsign=false commit -qm init
  # Three untracked files in three groups. After group 1 commits, groups 2 AND 3
  # are still untracked, so the message-only call for group 2 must exclude g3.
  printf 'G1_CONTENT\n' > "$d/g1.txt"
  printf 'G2_CONTENT\n' > "$d/g2.txt"
  printf 'G3_CONTENT\n' > "$d/g3.txt"
  printf '%s' '{"groups":[{"files":["g1.txt"],"summary":"g1","commit_message":"feat: g1"},{"files":["g2.txt"],"summary":"g2","commit_message":null},{"files":["g3.txt"],"summary":"g3","commit_message":null}]}' > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  # Run 2: cache hit, group 0 = g2 (null msg) -> message-only call scoped to g2,
  # while g3 is still untracked. The request body must contain g2 but not g3.
  : > "$CAPTURE"; : > "$PLAN_FILE"
  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
  grep -q 'G2_CONTENT' "$CAPTURE" && ok "scoped message includes the group's own untracked file" || bad "group 2 content missing from its message diff"
  grep -q 'G3_CONTENT' "$CAPTURE" && bad "another group's untracked content leaked into the message diff" || ok "other groups' untracked content excluded (filter works)"
  reset_cache; rm -rf "$d"
else
  skip "AC-C21 needs signing"
fi

stop_mock

# --- optional real-network smoke test --------------------------------------
if [ "${GCM_LIVE:-0}" = "1" ] && [ -n "${GROQ_API_KEY:-}" ]; then
  note "LIVE: real Groq call (GCM_LIVE=1)"
  if [ "$SIGNING_OK" -eq 1 ]; then
    d="$(new_repo)"; echo "live test change" > "$d/live.txt"
    ( cd "$d" && "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
    [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "live Groq -> one signed commit" || bad "live run (rc=$rc; $(tail -2 /tmp/gcm-out))"
    rm -rf "$d"
  else
    skip "live test needs working signing"
  fi
fi

printf '\n\033[1m== Summary ==\033[0m  PASS=%d FAIL=%d SKIP=%d\n' "$PASS" "$FAIL" "$SKIP"
[ "$FAIL" -eq 0 ]

codex
I’ve narrowed the likely issues to cache lifecycle semantics and fingerprint safety. I’m pulling exact line numbers now, along with the acceptance coverage gaps, so the final review can be concrete and source-backed.
exec
/bin/zsh -lc "nl -ba src/cache.rs | sed -n '150,230p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   150	
   151	/// The cache file name for a repo: `plan-<sha256(repo-root) hex>.json` (FR-25
   152	/// key). Pure - directory-independent, so the key/naming is unit-testable.
   153	fn cache_file_name(repo_root: &Path) -> String {
   154	    format!("plan-{}.json", repo_key(repo_root))
   155	}
   156	
   157	/// Hex SHA-256 of the absolute repo-root path (FR-25 cache key).
   158	fn repo_key(repo_root: &Path) -> String {
   159	    let mut h = Sha256::new();
   160	    h.update(repo_root.to_string_lossy().as_bytes());
   161	    hex(&h.finalize())
   162	}
   163	
   164	/// Fingerprint over the pending change set (FR-27): version + provider/model +
   165	/// per-file (path, content hash), with paths sorted for stability. Read from the
   166	/// LIVE change set each run; never pins `HEAD`; unborn-safe (working-tree reads
   167	/// + `git status` only).
   168	fn fingerprint(repo: &Repo, pending: &[ChangedFile], model: &str) -> String {
   169	    let mut entries: Vec<(String, String)> = pending
   170	        .iter()
   171	        .map(|f| (f.path.clone(), content_hash(repo, f)))
   172	        .collect();
   173	    entries.sort_by(|a, b| a.0.cmp(&b.0));
   174	    digest_fingerprint(model, &entries)
   175	}
   176	
   177	/// Combine pre-sorted `(path, content_hash)` entries into the fingerprint digest
   178	/// (pure; the fingerprint unit, testable without git or the filesystem).
   179	fn digest_fingerprint(model: &str, entries: &[(String, String)]) -> String {
   180	    let mut h = Sha256::new();
   181	    h.update(FINGERPRINT_VERSION.to_le_bytes());
   182	    h.update(b"\0");
   183	    h.update(PROVIDER.as_bytes());
   184	    h.update(b":");
   185	    h.update(model.as_bytes());
   186	    h.update(b"\0");
   187	    for (path, content) in entries {
   188	        h.update(path.as_bytes());
   189	        h.update(b"\0");
   190	        h.update(content.as_bytes());
   191	        h.update(b"\0");
   192	    }
   193	    hex(&h.finalize())
   194	}
   195	
   196	/// SHA-256 of a pending file's working-tree bytes, **streamed** in fixed-size
   197	/// chunks so a large binary still in `git status` cannot OOM the process. A
   198	/// pending deletion (file absent) is a distinct marker so a delete is detected;
   199	/// an unreadable file is its own marker (never silently equal to a real hash).
   200	fn content_hash(repo: &Repo, file: &ChangedFile) -> String {
   201	    let full = repo.root().join(&file.path);
   202	    let f = match fs::File::open(&full) {
   203	        Ok(f) => f,
   204	        Err(_) => return "\0DELETED".to_string(),
   205	    };
   206	    let mut hasher = Sha256::new();
   207	    let mut reader = BufReader::new(f);
   208	    let mut buf = [0u8; 64 * 1024];
   209	    loop {
   210	        match reader.read(&mut buf) {
   211	            Ok(0) => break,
   212	            Ok(n) => hasher.update(&buf[..n]),
   213	            Err(_) => return "\0UNREADABLE".to_string(),
   214	        }
   215	    }
   216	    hex(&hasher.finalize())
   217	}
   218	
   219	/// Lowercase hex encoding (avoids pulling in the `hex` crate).
   220	fn hex(bytes: &[u8]) -> String {
   221	    use std::fmt::Write as _;
   222	    let mut s = String::with_capacity(bytes.len() * 2);
   223	    for b in bytes {
   224	        let _ = write!(s, "{b:02x}");
   225	    }
   226	    s
   227	}
   228	
   229	fn serialize(cf: &CacheFile) -> io::Result<Vec<u8>> {
   230	    serde_json::to_vec_pretty(cf).map_err(io::Error::other)

exec
/bin/zsh -lc "nl -ba src/main.rs | sed -n '50,220p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
    50	
    51	    // Fail fast before sending any diff to the provider if we could not confirm
    52	    // the commit anyway (ADR-001 #10, AC-11).
    53	    if ui::needs_terminal_but_absent(args.yes, args.dry_run) {
    54	        return Err(GcmError::NonInteractive);
    55	    }
    56	
    57	    // Merge-state guard (CLO-487 review-2 #2) runs BEFORE any grouping bypass,
    58	    // including `--all`: staging a conflicted working tree on *either* path
    59	    // (grouping `add` or single-commit `add -A`) would bake `<<<<<<<` markers
    60	    // into the commit, so an unresolved conflict must abort regardless of flags.
    61	    let changed = repo.changed_files()?;
    62	    if changed.iter().any(|c| c.is_unmerged()) {
    63	        return Err(GcmError::UnmergedConflicts);
    64	    }
    65	
    66	    // `--all`, or a clean merge-in-progress, bypasses grouping and commits
    67	    // everything as one. A clean `MERGE_HEAD` makes `git commit` finalize the
    68	    // merge as a proper two-parent merge commit. The single-commit path clears
    69	    // the cached plan (FR-28).
    70	    if args.all || repo.is_merging() {
    71	        return single_commit(&repo, args);
    72	    }
    73	
    74	    // Grouping path. A fresh plan is persisted to the per-repo cache; a cache
    75	    // hit reuses it and skips the grouping call entirely (FR-25/FR-2). The
    76	    // model is folded into the freshness fingerprint (FR-27). A structured-
    77	    // output/parse/validation failure falls back to the single-commit path with
    78	    // an announced reason (never silent); a fatal error (missing key, git
    79	    // failure) is returned as-is.
    80	    let model = groq::resolved_model();
    81	    let plan = match cache::load(&repo, &model) {
    82	        Some(plan) => plan,
    83	        None => match build_plan(&repo, &changed) {
    84	            Ok(plan) => {
    85	                // Save the full plan even on a `--dry-run` (FR-7: dry-run
    86	                // uses/saves but does not advance); advancement is gated later.
    87	                cache::save(&repo, &plan, &model);
    88	                plan
    89	            }
    90	            Err(BuildError::Fatal(e)) => return Err(e),
    91	            Err(BuildError::Fallback(reason)) => {
    92	                eprintln!("gcm: {reason}. Falling back to single-commit mode.");
    93	                return single_commit(&repo, args);
    94	            }
    95	        },
    96	    };
    97	
    98	    commit_first_group(&repo, args, &changed, &plan, &model)
    99	}
   100	
   101	/// Whether the group-commit flow committed or the user aborted. Gates cache
   102	/// advancement: only a real commit advances the plan (FR-26) - never an abort.
   103	#[derive(Debug, PartialEq, Eq)]
   104	enum CommitOutcome {
   105	    Committed,
   106	    Aborted,
   107	}
   108	
   109	/// Outcome of a failed grouping attempt: `Fatal` errors abort (the single-commit
   110	/// path needs the same resource), `Fallback` errors degrade to single-commit.
   111	enum BuildError {
   112	    Fatal(GcmError),
   113	    Fallback(String),
   114	}
   115	
   116	/// Gather the grouping context, request the plan, and basic-validate it.
   117	/// Model/plan failures (structured-output error, unparseable JSON, empty
   118	/// response, validation) are `Fallback`; a missing key or git failure is
   119	/// `Fatal`.
   120	fn build_plan(repo: &Repo, changed: &[ChangedFile]) -> Result<Plan, BuildError> {
   121	    let ctx = diff::gather_for_grouping(repo, changed).map_err(BuildError::Fatal)?;
   122	    let plan = groq::generate_plan(&ctx).map_err(|e| match e {
   123	        // Missing key fails both paths identically; do not pretend to recover.
   124	        groq::GroqError::MissingKey => BuildError::Fatal(GcmError::Groq(e)),
   125	        other => BuildError::Fallback(other.to_string()),
   126	    })?;
   127	    let change_set: HashSet<String> = changed.iter().map(|c| c.path.clone()).collect();
   128	    plan::validate_basic(&plan, &change_set).map_err(|e| BuildError::Fallback(e.to_string()))?;
   129	    Ok(plan)
   130	}
   131	
   132	/// Display the groups, then (unless `--dry-run`) confirm and commit group 1,
   133	/// advancing the cache on a successful commit.
   134	fn commit_first_group(
   135	    repo: &Repo,
   136	    args: &Cli,
   137	    changed: &[ChangedFile],
   138	    plan: &Plan,
   139	    model: &str,
   140	) -> Result<(), GcmError> {
   141	    display_groups(plan);
   142	    let group1 = &plan.groups[0];
   143	    let group1_files = select_changed(changed, &group1.files);
   144	
   145	    // Resolve group 1's message. A fresh plan (or a full-plan cache hit) already
   146	    // carries it; an advanced cache hit has a null message, so regenerate it
   147	    // per group (ADR-001 #6) via a message-only call scoped to this group's diff,
   148	    // taken BEFORE staging. No grouping call is made here.
   149	    let message = match group1.commit_message.as_deref() {
   150	        Some(m) if !m.trim().is_empty() => m.to_string(),
   151	        _ => groq::generate_commit_message(&diff::gather_for_files(repo, &group1_files)?)?,
   152	    };
   153	
   154	    if args.dry_run {
   155	        ui::preview_plan(&message, plan.groups.len(), remaining_files(plan));
   156	        return Ok(());
   157	    }
   158	
   159	    // Capture the pre-run index up front. Restore it only on a *pre-commit-step*
   160	    // failure (FR-47). A commit-step failure (CommitFailed) leaves the group
   161	    // staged for retry (FR-58), so it is NOT restored. Abort never mutates the
   162	    // index, so it needs no restore.
   163	    let snapshot = repo.snapshot_index()?;
   164	    let result = commit_group_flow(repo, args, &group1_files, &message);
   165	    if let Err(e) = &result {
   166	        if !e.leaves_staged() {
   167	            let _ = repo.restore_index(&snapshot);
   168	        }
   169	    }
   170	
   171	    // Advance the cache only on a real commit - never on abort or failure.
   172	    if matches!(&result, Ok(CommitOutcome::Committed)) {
   173	        cache::advance(repo, plan, model);
   174	    }
   175	    result.map(|_| ())
   176	}
   177	
   178	/// Confirm, then clear staging and stage exactly group 1 before committing.
   179	fn commit_group_flow(
   180	    repo: &Repo,
   181	    args: &Cli,
   182	    group1_files: &[&ChangedFile],
   183	    message: &str,
   184	) -> Result<CommitOutcome, GcmError> {
   185	    match ui::confirm(message, args.yes)? {
   186	        Decision::Abort => {
   187	            println!("Aborted. Nothing staged, nothing committed.");
   188	            Ok(CommitOutcome::Aborted)
   189	        }
   190	        Decision::Commit(final_message) => {
   191	            repo.clear_staged()?;
   192	            repo.stage_group(group1_files)?;
   193	            repo.commit_signed(&final_message)?;
   194	            println!("Committed group 1.");
   195	            Ok(CommitOutcome::Committed)
   196	        }
   197	    }
   198	}
   199	
   200	/// The single-commit path (CLO-486 tracer): used by `--all`, a clean
   201	/// merge-in-progress, and the grouping fallback. Commits all changes as one.
   202	fn single_commit(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
   203	    if args.dry_run {
   204	        let gathered = diff::gather(repo)?;
   205	        let message = groq::generate_commit_message(&gathered)?;
   206	        ui_preview(&message);
   207	        return Ok(());
   208	    }
   209	    // `--all`, a clean merge, and the grouping fallback all clear the cached
   210	    // plan (FR-28); reached only on the real (non-dry-run) path.
   211	    cache::clear(repo);
   212	    let snapshot = repo.snapshot_index()?;
   213	    let result = single_commit_flow(repo, args);
   214	    if result.is_err() {
   215	        let _ = repo.restore_index(&snapshot);
   216	    }
   217	    result
   218	}
   219	
   220	fn single_commit_flow(repo: &Repo, args: &Cli) -> Result<(), GcmError> {

exec
/bin/zsh -lc "nl -ba scripts/acceptance.sh | sed -n '560,760p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   560	  git -C "$d" rev-parse HEAD >/dev/null 2>&1 && ok "HEAD now exists (first commit created)" || bad "no HEAD after run"
   561	  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'a.txt' && ok "group 1 (a.txt) committed" || bad "a.txt not committed"
   562	  [ -n "$(cache_file)" ] && ok "cache advanced to group 2" || bad "no cache after unborn first commit"
   563	  reset_cache; rm -rf "$d"
   564	else
   565	  skip "AC-C4 needs signing"
   566	fi
   567	
   568	note "AC-C5: cache file lives in the cache dir, named plan-<key>.json, mode 0600 (AC-5, FR-29)"
   569	if [ "$SIGNING_OK" -eq 1 ]; then
   570	  reset_cache; d="$(cache_repo_2group)"
   571	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   572	  cf="$(cache_file)"
   573	  [ -n "$cf" ] && [ -f "$cf" ] && ok "cache file created under the configured cache dir" || bad "no cache file produced"
   574	  case "$cf" in "$GCM_CACHE_DIR"/plan-*.json) ok "name is plan-<key>.json under GCM_CACHE_DIR" ;; *) bad "unexpected cache path: $cf" ;; esac
   575	  mode="$(stat -f '%Lp' "$cf" 2>/dev/null || stat -c '%a' "$cf" 2>/dev/null)"
   576	  [ "$mode" = "600" ] && ok "cache file mode is 0600" || bad "cache file mode is '$mode' (want 600)"
   577	  reset_cache; rm -rf "$d"
   578	else
   579	  skip "AC-C5 needs signing"
   580	fi
   581	
   582	note "AC-C6: --reset re-analyzes; --all clears the cache (AC-6, FR-8/FR-28)"
   583	if [ "$SIGNING_OK" -eq 1 ]; then
   584	  reset_cache; d="$(cache_repo_2group)"
   585	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   586	  [ -n "$(cache_file)" ] && ok "cache warmed (group 2 cached)" || bad "no cache after run 1"
   587	  : > "$CAPTURE"
   588	  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: d"}]}' > "$PLAN_FILE"
   589	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --reset --yes >/tmp/gcm-out 2>&1 )
   590	  grep -q '"response_format"' "$CAPTURE" && ok "--reset forced a grouping call" || bad "--reset did not re-analyze"
   591	  reset_cache; rm -rf "$d"
   592	
   593	  reset_cache; d="$(cache_repo_2group)"
   594	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   595	  [ -n "$(cache_file)" ] && ok "cache warmed before --all" || bad "no cache to clear"
   596	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 )
   597	  [ -z "$(cache_file)" ] && ok "--all cleared the cache" || bad "--all left the cache in place"
   598	  reset_cache; rm -rf "$d"
   599	else
   600	  skip "AC-C6 needs signing"
   601	fi
   602	
   603	note "AC-C7: aborting at the prompt leaves the cache un-advanced (AC-7)"
   604	if [ "$SIGNING_OK" -eq 1 ] && command -v expect >/dev/null 2>&1; then
   605	  reset_cache; d="$(cache_repo_2group)"
   606	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   607	  before="$(cat "$(cache_file)")"
   608	  GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" GCM_BIN="$BIN" GCM_DIR="$d" expect -c '
   609	    set timeout 20
   610	    spawn -noecho sh -c "cd $env(GCM_DIR) && GROQ_API_KEY=$env(GROQ_API_KEY) GCM_GROQ_BASE_URL=$env(GCM_GROQ_BASE_URL) $env(GCM_BIN)"
   611	    expect {
   612	      -re {\[Y/n/e} { send "n\r" }
   613	      timeout { exit 3 }
   614	    }
   615	    expect eof
   616	  ' >/tmp/gcm-out 2>&1
   617	  after="$(cat "$(cache_file)")"
   618	  [ "$before" = "$after" ] && ok "cache byte-identical after abort (not advanced)" || bad "abort changed/advanced the cache"
   619	  git -C "$d" status --porcelain | grep -q 'docs.md' && ok "group 2 still pending after abort" || bad "group 2 not pending after abort"
   620	  reset_cache; rm -rf "$d"
   621	else
   622	  skip "AC-C7 needs signing + expect"
   623	fi
   624	
   625	note "AC-C11: a single-group plan deletes the cache after its commit (eval 11)"
   626	if [ "$SIGNING_OK" -eq 1 ]; then
   627	  reset_cache; d="$(new_repo)"
   628	  printf 'v1\n' > "$d/only.txt"
   629	  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
   630	  git -C "$d" -c commit.gpgsign=false commit -qm init
   631	  printf 'v2\n' > "$d/only.txt"
   632	  printf '%s' '{"groups":[{"files":["only.txt"],"summary":"only","commit_message":"feat: only"}]}' > "$PLAN_FILE"
   633	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   634	  [ -z "$(cache_file)" ] && ok "single-group plan left no cache (nothing to advance to)" || bad "cache lingered after the last group"
   635	  reset_cache; rm -rf "$d"
   636	else
   637	  skip "AC-C11 needs signing"
   638	fi
   639	
   640	note "AC-C21: a group's message excludes other groups' untracked files (blind-spot #3, eval 21)"
   641	if [ "$SIGNING_OK" -eq 1 ]; then
   642	  reset_cache; d="$(new_repo)"
   643	  printf 'seed\n' > "$d/seed.txt"
   644	  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
   645	  git -C "$d" -c commit.gpgsign=false commit -qm init
   646	  # Three untracked files in three groups. After group 1 commits, groups 2 AND 3
   647	  # are still untracked, so the message-only call for group 2 must exclude g3.
   648	  printf 'G1_CONTENT\n' > "$d/g1.txt"
   649	  printf 'G2_CONTENT\n' > "$d/g2.txt"
   650	  printf 'G3_CONTENT\n' > "$d/g3.txt"
   651	  printf '%s' '{"groups":[{"files":["g1.txt"],"summary":"g1","commit_message":"feat: g1"},{"files":["g2.txt"],"summary":"g2","commit_message":null},{"files":["g3.txt"],"summary":"g3","commit_message":null}]}' > "$PLAN_FILE"
   652	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   653	  # Run 2: cache hit, group 0 = g2 (null msg) -> message-only call scoped to g2,
   654	  # while g3 is still untracked. The request body must contain g2 but not g3.
   655	  : > "$CAPTURE"; : > "$PLAN_FILE"
   656	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   657	  grep -q 'G2_CONTENT' "$CAPTURE" && ok "scoped message includes the group's own untracked file" || bad "group 2 content missing from its message diff"
   658	  grep -q 'G3_CONTENT' "$CAPTURE" && bad "another group's untracked content leaked into the message diff" || ok "other groups' untracked content excluded (filter works)"
   659	  reset_cache; rm -rf "$d"
   660	else
   661	  skip "AC-C21 needs signing"
   662	fi
   663	
   664	stop_mock
   665	
   666	# --- optional real-network smoke test --------------------------------------
   667	if [ "${GCM_LIVE:-0}" = "1" ] && [ -n "${GROQ_API_KEY:-}" ]; then
   668	  note "LIVE: real Groq call (GCM_LIVE=1)"
   669	  if [ "$SIGNING_OK" -eq 1 ]; then
   670	    d="$(new_repo)"; echo "live test change" > "$d/live.txt"
   671	    ( cd "$d" && "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   672	    [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "live Groq -> one signed commit" || bad "live run (rc=$rc; $(tail -2 /tmp/gcm-out))"
   673	    rm -rf "$d"
   674	  else
   675	    skip "live test needs working signing"
   676	  fi
   677	fi
   678	
   679	printf '\n\033[1m== Summary ==\033[0m  PASS=%d FAIL=%d SKIP=%d\n' "$PASS" "$FAIL" "$SKIP"
   680	[ "$FAIL" -eq 0 ]

exec
/bin/zsh -lc 'rg -n "rename|reformats|restage|deletion-only|untracked-only|AC-C21|AC-C11|AC-C[1-7]" scripts/acceptance.sh docs/design-docs/2026-06-20-clo-491-plan-cache.md' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
scripts/acceptance.sh:330:note "AC-G4: rename + delete + ->-in-name + unicode group without fallback"
scripts/acceptance.sh:336:  git -C "$d" mv orig.txt renamed.txt
scripts/acceptance.sh:340:  printf '%s' '{"groups":[{"files":["renamed.txt","del.txt","a -> b.txt","файл.txt"],"summary":"mixed","commit_message":"chore: reshuffle files"}]}' > "$PLAN_FILE"
scripts/acceptance.sh:344:  git -C "$d" -c core.quotePath=false ls-files | grep -q 'renamed.txt' && ok "rename new path committed" || bad "rename new path missing"
scripts/acceptance.sh:345:  git -C "$d" -c core.quotePath=false ls-files | grep -q 'orig.txt' && bad "rename old path still tracked" || ok "rename old path deleted (rename completed)"
scripts/acceptance.sh:499:note "AC-C1: re-run commits group 2 from cache with no grouping call (AC-1, FR-2)"
scripts/acceptance.sh:514:  skip "AC-C1 needs signing"
scripts/acceptance.sh:517:note "AC-C2: editing a pending file invalidates the cache and re-analyzes (AC-2, FR-27)"
scripts/acceptance.sh:529:  skip "AC-C2 needs signing"
scripts/acceptance.sh:532:note "AC-C3: rejecting pre-commit hook leaves the group staged + plan un-advanced (AC-3, FR-58)"
scripts/acceptance.sh:549:  skip "AC-C3 retry-commit assertion needs signing"
scripts/acceptance.sh:553:note "AC-C4: first commit in an unborn repo (no HEAD) works with the cache (AC-4)"
scripts/acceptance.sh:565:  skip "AC-C4 needs signing"
scripts/acceptance.sh:568:note "AC-C5: cache file lives in the cache dir, named plan-<key>.json, mode 0600 (AC-5, FR-29)"
scripts/acceptance.sh:579:  skip "AC-C5 needs signing"
scripts/acceptance.sh:582:note "AC-C6: --reset re-analyzes; --all clears the cache (AC-6, FR-8/FR-28)"
scripts/acceptance.sh:600:  skip "AC-C6 needs signing"
scripts/acceptance.sh:603:note "AC-C7: aborting at the prompt leaves the cache un-advanced (AC-7)"
scripts/acceptance.sh:622:  skip "AC-C7 needs signing + expect"
scripts/acceptance.sh:625:note "AC-C11: a single-group plan deletes the cache after its commit (eval 11)"
scripts/acceptance.sh:637:  skip "AC-C11 needs signing"
scripts/acceptance.sh:640:note "AC-C21: a group's message excludes other groups' untracked files (blind-spot #3, eval 21)"
scripts/acceptance.sh:661:  skip "AC-C21 needs signing"
docs/design-docs/2026-06-20-clo-491-plan-cache.md:150:  before writing the plan**, then rename over the target (`#[cfg(unix)]`). Setting the mode
docs/design-docs/2026-06-20-clo-491-plan-cache.md:280:error to the user (FR-58 "surface the error"). A pre-commit hook that *reformats and
docs/design-docs/2026-06-20-clo-491-plan-cache.md:358:- [ ] `load` (deserialize + format-version check + fingerprint match), `save` (atomic temp+rename, 0600), `advance`, `clear`
docs/design-docs/2026-06-20-clo-491-plan-cache.md:379:  an advanced group - but `gather_for_files` must be correct for the untracked-only case
docs/design-docs/2026-06-20-clo-491-plan-cache.md:411:  staging, rename = new path) - the cache reuses `changed_files()` output, it does not
docs/design-docs/2026-06-20-clo-491-plan-cache.md:433:- Prefer atomic writes (temp + rename) over in-place truncation.
docs/design-docs/2026-06-20-clo-491-plan-cache.md:484:| 6 | Hook reformats+re-stages -> commit succeeds, advance | commit lands; cache advances to g2 | acceptance: hook that edits+`git add`+exit 0 |
docs/design-docs/2026-06-20-clo-491-plan-cache.md:495:| 17 | Group 0 is a deletion-only group | message-only call gets the deletion diff; commit records the removal; cache advances | acceptance: plan whose group 0 deletes a file, cache hit |
docs/design-docs/2026-06-20-clo-491-plan-cache.md:496:| 18 | Group 0 is untracked-only | `gather_for_files` includes only that group's untracked content; message generated; commit adds the new file | acceptance: plan whose group 0 is a new untracked file, cache hit |
docs/design-docs/2026-06-20-clo-491-plan-cache.md:505:- Concurrent runs in the same repo (last writer wins; atomic rename prevents a torn file).

exec
/bin/zsh -lc "nl -ba src/git.rs | sed -n '100,150p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
exec
/bin/zsh -lc "nl -ba src/git.rs | sed -n '200,240p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   100	        if self.has_head() {
   101	            self.capture(&["diff", "--stat", "HEAD"])
   102	        } else {
   103	            let unstaged = self.capture(&["diff", "--stat"])?;
   104	            let staged = self.capture(&["diff", "--cached", "--stat"])?;
   105	            Ok(format!("{unstaged}{staged}"))
   106	        }
   107	    }
   108	
   109	    /// Full diff (no color) for the prompt body. HEAD when present; otherwise
   110	    /// unstaged + staged on an unborn branch. See [`Self::diff_stat`] for the
   111	    /// unborn-branch rationale.
   112	    pub fn diff_full(&self) -> Result<String, GcmError> {
   113	        if self.has_head() {
   114	            self.capture(&["diff", "--no-color", "HEAD"])
   115	        } else {
   116	            let unstaged = self.capture(&["diff", "--no-color"])?;
   117	            let staged = self.capture(&["diff", "--no-color", "--cached"])?;
   118	            Ok(format!("{unstaged}{staged}"))
   119	        }
   120	    }
   121	
   122	    /// Diff `--stat` scoped to specific paths (CLO-491 per-group message header).
   123	    /// Same HEAD/unborn handling as [`Self::diff_stat`]. Empty `paths` returns an
   124	    /// empty string rather than an unscoped whole-tree diff.
   125	    pub fn diff_stat_for(&self, paths: &[&str]) -> Result<String, GcmError> {
   126	        if paths.is_empty() {
   127	            return Ok(String::new());
   128	        }
   129	        if self.has_head() {
   130	            self.capture_scoped(&["diff", "--stat", "HEAD"], paths)
   131	        } else {
   132	            let unstaged = self.capture_scoped(&["diff", "--stat"], paths)?;
   133	            let staged = self.capture_scoped(&["diff", "--stat", "--cached"], paths)?;
   134	            Ok(format!("{unstaged}{staged}"))
   135	        }
   136	    }
   137	
   138	    /// Full diff (no color) scoped to specific paths (CLO-491 per-group message
   139	    /// body). Same HEAD/unborn handling as [`Self::diff_full`]. Empty `paths`
   140	    /// returns an empty string.
   141	    pub fn diff_full_for(&self, paths: &[&str]) -> Result<String, GcmError> {
   142	        if paths.is_empty() {
   143	            return Ok(String::new());
   144	        }
   145	        if self.has_head() {
   146	            self.capture_scoped(&["diff", "--no-color", "HEAD"], paths)
   147	        } else {
   148	            let unstaged = self.capture_scoped(&["diff", "--no-color"], paths)?;
   149	            let staged = self.capture_scoped(&["diff", "--no-color", "--cached"], paths)?;
   150	            Ok(format!("{unstaged}{staged}"))

 succeeded in 0ms:
   200	
   201	    /// Restore the index to a previously-snapshotted tree. The working tree is
   202	    /// untouched; this only rewinds staging (FR-47 restore on abort/failure).
   203	    pub fn restore_index(&self, tree: &str) -> Result<(), GcmError> {
   204	        self.capture(&["read-tree", tree]).map(|_| ())
   205	    }
   206	
   207	    /// Stage every change (the tracer commits all changes as one commit, FR-6).
   208	    pub fn stage_all(&self) -> Result<(), GcmError> {
   209	        self.capture(&["add", "-A"]).map(|_| ())
   210	    }
   211	
   212	    /// Create a signed commit (FR-4). Stdio is inherited so GPG/SSH passphrase
   213	    /// (pinentry) prompts work on the user's terminal.
   214	    ///
   215	    /// A non-zero `git commit` (a rejecting pre-commit hook, a signing failure)
   216	    /// returns [`GcmError::CommitFailed`], not [`GcmError::Git`]: the caller
   217	    /// leaves the staged group in place and does not advance the plan cache
   218	    /// (CLO-491, FR-58). A failure to even spawn `git` is a `Git` error (no
   219	    /// commit was attempted, so the staged group should be rolled back).
   220	    pub fn commit_signed(&self, message: &str) -> Result<(), GcmError> {
   221	        let status = self
   222	            .git(&["commit", "-S", "-m", message])
   223	            .stdin(Stdio::inherit())
   224	            .stdout(Stdio::inherit())
   225	            .stderr(Stdio::inherit())
   226	            .status()
   227	            .map_err(|e| GcmError::Git(format!("failed to run git commit: {e}")))?;
   228	        if !status.success() {
   229	            return Err(GcmError::CommitFailed(
   230	                "git commit failed (see output above)".to_string(),
   231	            ));
   232	        }
   233	        Ok(())
   234	    }
   235	
   236	    /// The full changed-file set for grouping, from
   237	    /// `git status --porcelain=v1 -uall -z`. `-uall` expands untracked
   238	    /// directories to individual files so these paths match the per-file diff
   239	    /// paths (CLO-487 review-2 #1). NUL-delimited; renames carry their orig path.
   240	    pub fn changed_files(&self) -> Result<Vec<ChangedFile>, GcmError> {

exec
/bin/zsh -lc "nl -ba docs/design-docs/2026-06-20-clo-491-plan-cache.md | sed -n '470,505p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   470	cargo test && ./scripts/acceptance.sh` - all green; the new acceptance cases AC-1..AC-6
   471	pass.
   472	
   473	---
   474	
   475	## Evaluation
   476	
   477	| # | Test | Expected Result | Command / Steps |
   478	|---|------|-----------------|-----------------|
   479	| 1 | Cache hit advances to group 2, no grouping call | group 2 committed; mock records 0 grouping calls, 1 message call; valid message | acceptance: 2-group change, commit g1, re-run |
   480	| 2 | Full-plan hit after dry-run needs zero LLM calls | real run commits g1 with the cached message; 0 LLM calls on the real run | acceptance: `--dry-run` then real run, same tree |
   481	| 3 | Edit a pending file -> re-analyze | run 2 records a fresh grouping call | acceptance: commit g1, edit a g2 file, re-run |
   482	| 4 | Rename a pending file -> re-analyze | fingerprint mismatch -> grouping call | acceptance: commit g1, `git mv` a g2 file, re-run |
   483	| 5 | Pre-commit hook rejects -> un-advanced, staged | exit!=0; g1 staged; cache file byte-identical; next run retries g1 | acceptance: install `exit 1` hook |
   484	| 6 | Hook reformats+re-stages -> commit succeeds, advance | commit lands; cache advances to g2 | acceptance: hook that edits+`git add`+exit 0 |
   485	| 7 | Unborn-branch first commit with cache | g1 commits; cache saved then advanced; no HEAD errors | acceptance: fresh `git init`, 2-group tree |
   486	| 8 | Cache file perms + location | path under OS cache dir; mode 0600 (Unix) | unit/acceptance: assert `cache_path` parent + `stat -f %Lp` |
   487	| 9 | `--reset` forces re-analysis | cache deleted up front; grouping call made | acceptance: warm cache, run `--reset` |
   488	| 10 | `--all` / fallback clears cache | cache file absent after run | acceptance: warm cache, run `--all`; malformed-plan fallback |
   489	| 11 | Single-group plan -> cache deleted after commit | no cache file remains (nothing to advance to) | acceptance: 1-group change, commit |
   490	| 12 | Corrupt cache file -> treated as miss | re-analyzes; no panic | unit: write garbage to the cache path, `load` returns `None` |
   491	| 13 | Format-version bump -> miss | old-version file ignored, re-analyzes | unit: write `version: 0`, `load` returns `None` |
   492	| 14 | Provider/model change -> re-analyze | switching `GCM_GROQ_MODEL` mismatches the fingerprint | unit: fingerprint differs across model strings |
   493	| 15 | Advance-write failure self-heals | next run re-analyzes (live fingerprint != stored) | reasoned/unit: simulate via stale fingerprint |
   494	| 16 | Deleted pending file folds into fingerprint | delete marker distinguishes present vs deleted | unit: `content_hash` for a deletion |
   495	| 17 | Group 0 is a deletion-only group | message-only call gets the deletion diff; commit records the removal; cache advances | acceptance: plan whose group 0 deletes a file, cache hit |
   496	| 18 | Group 0 is untracked-only | `gather_for_files` includes only that group's untracked content; message generated; commit adds the new file | acceptance: plan whose group 0 is a new untracked file, cache hit |
   497	| 19 | User **aborts** at the prompt -> cache NOT advanced | cache file byte-identical; next run offers the same group 0 | acceptance: warm cache, answer `n` at confirm |
   498	| 20 | Large pending binary -> no OOM | fingerprint computed with bounded memory; process does not crash | unit/acceptance: a >100 MB pending file, `content_hash` streams |
   499	| 21 | Untracked filter: group 0's message excludes other groups' untracked files | message diff contains only group 0's untracked paths | unit: `gather_for_files` with a multi-group untracked set |
   500	
   501	**Edge cases to cover**:
   502	- Provider unset / `GROQ_API_KEY` absent during fingerprinting (use `resolved_model`, which
   503	  does not require the key).
   504	- Cache dir does not yet exist (create it; do not fail).
   505	- Concurrent runs in the same repo (last writer wins; atomic rename prevents a torn file).

exec
/bin/zsh -lc "nl -ba scripts/acceptance.sh | sed -n '530,550p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   530	fi
   531	
   532	note "AC-C3: rejecting pre-commit hook leaves the group staged + plan un-advanced (AC-3, FR-58)"
   533	reset_cache; d="$(cache_repo_2group)"
   534	mkdir -p "$d/.git/hooks"
   535	printf '#!/bin/sh\nexit 1\n' > "$d/.git/hooks/pre-commit"; chmod +x "$d/.git/hooks/pre-commit"
   536	( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   537	[ $rc -ne 0 ] && ok "rejecting hook -> exit $rc" || bad "expected non-zero on hook rejection"
   538	grep -qi "left staged" /tmp/gcm-out && ok "error explains the group is left staged" || bad "FR-58 message missing"
   539	git -C "$d" diff --cached --name-only | grep -qx 'src.txt' && ok "group 1 left staged for retry" || bad "group 1 not staged after hook reject"
   540	cf="$(cache_file)"
   541	[ -n "$cf" ] && grep -q '"src.txt"' "$cf" && ok "cache un-advanced (still holds group 1)" || bad "cache advanced despite the commit failure"
   542	[ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "no commit created" || bad "a commit slipped through the rejecting hook"
   543	# Removing the hook and re-running retries the same group from the cache.
   544	rm -f "$d/.git/hooks/pre-commit"; : > "$CAPTURE"; : > "$PLAN_FILE"
   545	( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   546	if [ "$SIGNING_OK" -eq 1 ]; then
   547	  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'src.txt' && ok "retry committed the same group 1 from cache" || bad "retry did not commit group 1"
   548	else
   549	  skip "AC-C3 retry-commit assertion needs signing"
   550	fi

exec
/bin/zsh -lc "nl -ba scripts/acceptance.sh | sed -n '490,530p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   490	  local d; d="$(new_repo)"
   491	  printf 'v1\n' > "$d/src.txt"; printf 'v1\n' > "$d/docs.md"
   492	  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
   493	  git -C "$d" -c commit.gpgsign=false commit -qm init
   494	  printf 'v2\n' > "$d/src.txt"; printf 'v2\n' > "$d/docs.md"
   495	  printf '%s' '{"groups":[{"files":["src.txt"],"summary":"source","commit_message":"feat: src"},{"files":["docs.md"],"summary":"docs","commit_message":null}]}' > "$PLAN_FILE"
   496	  echo "$d"
   497	}
   498	
   499	note "AC-C1: re-run commits group 2 from cache with no grouping call (AC-1, FR-2)"
   500	if [ "$SIGNING_OK" -eq 1 ]; then
   501	  reset_cache; d="$(cache_repo_2group)"
   502	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   503	  # Run 2 is a cache hit: capture only this run, and blank the plan so any
   504	  # (unexpected) grouping call would be visible as a fallback.
   505	  : > "$CAPTURE"; : > "$PLAN_FILE"
   506	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   507	  [ $rc -eq 0 ] && ok "re-run exit 0" || bad "re-run (rc=$rc; $(tail -1 /tmp/gcm-out))"
   508	  grep -q '"response_format"' "$CAPTURE" && bad "re-run made a grouping call (cache missed)" || ok "no grouping call on re-run (cache hit)"
   509	  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'docs.md' && ok "group 2 committed from cache" || bad "group 2 not committed"
   510	  git -C "$d" log -1 --pretty=%s | grep -qi "mock commit message" && ok "group 2 carried a valid (regenerated) message" || bad "group 2 message missing"
   511	  [ -z "$(git -C "$d" status --porcelain)" ] && ok "tree clean after group 2" || bad "tree still dirty"
   512	  reset_cache; rm -rf "$d"
   513	else
   514	  skip "AC-C1 needs signing"
   515	fi
   516	
   517	note "AC-C2: editing a pending file invalidates the cache and re-analyzes (AC-2, FR-27)"
   518	if [ "$SIGNING_OK" -eq 1 ]; then
   519	  reset_cache; d="$(cache_repo_2group)"
   520	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   521	  printf 'v3-edited\n' > "$d/docs.md"   # edit the still-pending group-2 file
   522	  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: edited"}]}' > "$PLAN_FILE"
   523	  : > "$CAPTURE"
   524	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   525	  [ $rc -eq 0 ] && ok "re-run after edit exit 0" || bad "edit re-run (rc=$rc)"
   526	  grep -q '"response_format"' "$CAPTURE" && ok "edit invalidated the cache -> grouping call" || bad "stale cache reused after a content edit"
   527	  reset_cache; rm -rf "$d"
   528	else
   529	  skip "AC-C2 needs signing"
   530	fi

codex
## Verdict: FAIL

## Findings
- HIGH — [src/cache.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/cache.rs:200): `content_hash()` follows symlinks and reads whatever the target is. For a pending symlink, the fingerprint is based on the target bytes, not the symlink entry Git would commit; for a symlink to a FIFO/device/socket, this can block indefinitely. That violates the CLO-491 safe-path/symlink handling requirement and can cause false cache invalidation or hangs.
- MEDIUM — [src/main.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/main.rs:203): the single-commit path returns from the `--dry-run` branch before `cache::clear()`, so `--all --dry-run` and dry-run fallback do not clear the cache. The design/plan says `--all` and the single-commit fallback clear it, not only the real-commit branch.

## Missing Items
- The named CLO-491 acceptance cases are present in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:499): `AC-C1`..`AC-C7`, `AC-C11`, and `AC-C21`.
- Coverage is still missing for design eval row 4: rename a pending file after group 1 commit should force re-analysis.
- Coverage is still missing for design eval row 6: pre-commit hook reformats and re-stages, commit succeeds, cache advances.
- Coverage is still missing for design eval rows 17 and 18: deletion-only cached group and untracked-only cached group.
- Coverage is still missing for design eval row 20 and the security checklist around `content_hash()`: large-file streaming, symlink, and special-file behavior.
- The fallback half of eval row 10 is untested: [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:582) checks `--reset` and real `--all`, but not “malformed-plan fallback clears the cache”.
- `AC-C3` is weaker than the design text: [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:540) only checks that the cache still contains `src.txt`, not that the cache file is byte-identical before/after the rejected commit.

## Recommendations
- Fix `content_hash()` to inspect `symlink_metadata()` first: hash regular files, hash symlink payloads if you want symlinks included, and return explicit markers for deleted/unreadable/non-regular entries without following special files.
- Move cache clearing ahead of the `single_commit()` dry-run early return, or clear at the call sites before entering any single-commit mode.
- Add targeted tests for rename invalidation, hook reformat+restage advancement, deletion-only and untracked-only cached groups, malformed-plan fallback clearing, and `content_hash()` on symlinks/special files/large files.
- Re-run `cargo test` and `./scripts/acceptance.sh` after the fixes; this review was static only.
hook: Stop
hook: Stop Completed
tokens used
186,940
## Verdict: FAIL

## Findings
- HIGH — [src/cache.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/cache.rs:200): `content_hash()` follows symlinks and reads whatever the target is. For a pending symlink, the fingerprint is based on the target bytes, not the symlink entry Git would commit; for a symlink to a FIFO/device/socket, this can block indefinitely. That violates the CLO-491 safe-path/symlink handling requirement and can cause false cache invalidation or hangs.
- MEDIUM — [src/main.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/main.rs:203): the single-commit path returns from the `--dry-run` branch before `cache::clear()`, so `--all --dry-run` and dry-run fallback do not clear the cache. The design/plan says `--all` and the single-commit fallback clear it, not only the real-commit branch.

## Missing Items
- The named CLO-491 acceptance cases are present in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:499): `AC-C1`..`AC-C7`, `AC-C11`, and `AC-C21`.
- Coverage is still missing for design eval row 4: rename a pending file after group 1 commit should force re-analysis.
- Coverage is still missing for design eval row 6: pre-commit hook reformats and re-stages, commit succeeds, cache advances.
- Coverage is still missing for design eval rows 17 and 18: deletion-only cached group and untracked-only cached group.
- Coverage is still missing for design eval row 20 and the security checklist around `content_hash()`: large-file streaming, symlink, and special-file behavior.
- The fallback half of eval row 10 is untested: [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:582) checks `--reset` and real `--all`, but not “malformed-plan fallback clears the cache”.
- `AC-C3` is weaker than the design text: [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:540) only checks that the cache still contains `src.txt`, not that the cache file is byte-identical before/after the rejected commit.

## Recommendations
- Fix `content_hash()` to inspect `symlink_metadata()` first: hash regular files, hash symlink payloads if you want symlinks included, and return explicit markers for deleted/unreadable/non-regular entries without following special files.
- Move cache clearing ahead of the `single_commit()` dry-run early return, or clear at the call sites before entering any single-commit mode.
- Add targeted tests for rename invalidation, hook reformat+restage advancement, deletion-only and untracked-only cached groups, malformed-plan fallback clearing, and `content_hash()` on symlinks/special files/large files.
- Re-run `cargo test` and `./scripts/acceptance.sh` after the fixes; this review was static only.
