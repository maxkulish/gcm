# CLO-491 Implementation Plan: Per-repo plan cache with commit-safe advancement

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-491/add-per-repo-plan-cache-with-commit-safe-advancement
**Design Document**: docs/design-docs/2026-06-20-clo-491-plan-cache.md
**Workflow State**: docs/status/clo-491-workflow.yaml
**Created**: 2026-06-20
**Overall Progress**: 94% (Tasks 1-16 complete incl. dual-model validation; Phase 5 PR in progress)

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
