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
