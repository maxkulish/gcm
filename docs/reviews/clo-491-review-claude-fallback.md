# Design Review: CLO-491 - Per-repo plan cache with commit-safe advancement

**Reviewed**: 2026-06-20
**Reviewer**: Claude (Opus 4.8, 1M context) - source-validated review
**Design Document**: docs/design-docs/2026-06-20-clo-491-plan-cache.md
**Context**: ADR-001, ROADMAP, DEPENDENCIES, PROJECT, plus full read of src/{main,git,groq,diff,plan,cli,error}.rs

> External-model reviewers (Gemini/Ollama) were not invoked in this run; this is a single
> source-validated review. Every interface claim in the design was checked against the
> actual code on the branch.

---

## 1. Completeness Check

| Section | Present | Assessment |
|---------|---------|------------|
| Summary | Yes | Clear problem + the two bash defects it fixes (name-only staleness, null-message advancement). |
| Background | Yes | Strong - cites the exact bash line ranges and ties the message contract to ADR-001 #6. |
| Architecture | Yes | Component table, dependency list, and an ASCII flow that maps the cache into the existing `execute` path. |
| Detailed Design | Yes | Cache format, key, fingerprint, message-on-hit, commit-safe advancement, lifecycle table, resilience. Thorough. |
| Implementation Plan | Yes | 4 phases, each a checklist; maps cleanly to the affected-components table. |
| Constraints | Yes | Must / Must-not / Prefer / Escalate - unusually well-formed. |
| Acceptance Criteria | Yes | AC-1..AC-6 with explicit verification method; a 16-row Evaluation matrix and an edge-case list. |
| Testing Strategy | Yes | Unit + acceptance + manual, reusing the CLO-487 mock-Groq harness. |
| Open Questions | Yes | The "no new analysis call" interpretation is flagged for sign-off (correctly - see below). |

Nothing structural is missing. This is one of the more complete design docs in the set.

## 2. Architecture Assessment

**Strengths**

- **Interface claims are accurate.** I verified every primitive the design says it will
  reuse. They all exist with the stated signatures:
  - `Repo::root()` -> `&Path` (git.rs:38), `has_head()` (git.rs:68), `changed_files()`
    (git.rs:181), `commit_signed()` (git.rs:161), `snapshot_index`/`restore_index`
    (git.rs:144/150), `clear_staged` (git.rs:211), `stage_group` (git.rs:226).
  - `groq::generate_commit_message(&GatheredDiff)` (groq.rs:138) exists and is the right
    reuse target for the per-group message.
  - `Plan`/`Group` are `#[derive(Debug, Deserialize)]` only (plan.rs:10,17) - the design
    correctly identifies that it must add `Serialize`. `commit_message: Option<String>`
    already serializes `null` faithfully.
  - `cli.rs` has no `--reset` today (only `--dry_run`, `--all`, `--yes`) - the design
    correctly lists adding it as new work.
- **Fingerprint-over-content is the right fix** and is well specified: live `git status`
  pending set + per-file working-tree content hash + provider/model + version, never
  pinning HEAD. The "no HEAD pin" rationale (committing group 1 must not self-invalidate
  the remainder) is exactly right and is the crux of FR-26.
- **Best-effort cache I/O** (corrupt -> miss, write failure -> warn-and-continue, advance
  self-heals via fingerprint mismatch) is the correct posture and is consistent with
  ADR-001 #1 (Rust owns the file, the file is the state).
- **ADR-001 alignment is strong**: synchronous module, no async (ADR-001 #2), OS cache dir
  via `directories` (#12), regenerate-per-group messages (#6), provider-agnostic key with
  provider folded into the fingerprint (#12 / FR-25). No new ADR is warranted.

**Concerns** (ordered by severity)

- **[HIGH] The FR-58 "leave staged, surface error" path is under-specified at the git
  layer.** The design narrows the *wrapper* in `main.rs` (don't restore on `CommitFailed`),
  but the failure actually originates in `git.rs::commit_signed`, which today returns
  `GcmError::Git("git commit failed (see output above); index restored")` (git.rs:161-175).
  Two problems the design does not call out:
  1. That error message literally says "index restored" - which will now be **false** on
     the FR-58 path (the whole point is to *not* restore). The design adds
     `GcmError::CommitFailed` but never says `commit_signed` must *return* it (the affected-
     components table only mentions `error.rs` and the `main.rs` wrapper). If `commit_signed`
     keeps returning `GcmError::Git(..)`, then `leaves_staged()` (which the design defines as
     `true` only for `CommitFailed`) returns `false`, the wrapper **restores the index**, and
     **AC-3 fails** (the group would be unstaged, not left staged). This is the single most
     important correctness gap: `commit_signed` must be changed to return `CommitFailed`,
     and its "index restored" wording removed/corrected. Phase 3's `main.rs` bullet should
     explicitly include `git.rs::commit_signed` returning the new variant.
  2. The design's `commit_group_flow` pseudocode wraps `commit_signed` and converts `Err`
     into `GcmError::CommitFailed(..)`. That is plausible, but it must be the *commit step
     specifically* - `clear_staged()` and `stage_group()` also return `GcmError::Git` and
     must NOT be mistaken for `CommitFailed` (they should restore, FR-47). The pseudocode
     shows this intent but the mechanism (how `commit_signed`'s error is distinguished from
     a `stage_group` error when both are `GcmError::Git`) needs to be concrete. Cleanest:
     `commit_signed` returns `CommitFailed`; `clear_staged`/`stage_group` keep returning
     `Git`. Then `leaves_staged()` discriminates correctly with no string matching.

- **[MEDIUM] Message-only diff scoping vs the now-staged index.** On a cache hit for an
  *advanced* group (cached message `null`), the design generates the message via
  `diff::gather_for_files` scoped to group 0's files, then commits. But note the existing
  flow stages group 0 *inside* `commit_group_flow` (clear_staged -> stage_group ->
  commit_signed). `gather_for_files` must read the diff for those files **from the working
  tree** (HEAD/unborn, like `diff_full`), independent of staging order, and must be called
  with the right files. The design says "scoped to that group's diff" but does not pin down
  whether the message is generated *before* staging (working-tree diff) or *after* (index
  diff). Recommend: generate the message from the working-tree diff of group 0's files
  *before* `clear_staged`, mirroring how the miss path already has `group1.commit_message`
  in hand before staging. State this ordering explicitly so the diff source is deterministic
  and matches CLO-487's "stage from working tree" model.

- **[MEDIUM] `gather_for_files` is genuinely new and non-trivial - the plan under-scopes
  it.** I confirmed `diff_full`/`diff_stat` (git.rs:99-120) take no pathspec; they diff the
  whole tree. So `gather_for_files` needs either a new path-scoped git diff
  (`git diff HEAD -- <paths>`, unborn-safe via the same unstaged+staged split) *plus*
  untracked-file handling for any group-0 untracked files (an untracked file has no diff -
  CLO-487's `append_untracked` reads its content). The design lists `gather_for_files` as a
  single Phase-2 checkbox and says "untracked/unborn safe" in the API table, but the
  untracked-in-a-group case (a brand-new file that is the only change in group 0) is exactly
  AC-4's territory and deserves an explicit sub-step. Underestimating this is the most
  likely schedule risk.

- **[LOW] `resolved_model()` placement and the fingerprint's model string.** The design adds
  `groq::resolved_model()` that returns the model "without requiring the API key". Today
  `resolve_config()` (groq.rs:82-96) reads key+model+base_url together and errors on a
  missing key. Factoring out just the model resolution is clean and correct, but note the
  fingerprint string is `"groq:<model>"` (hard-coded provider prefix). Since the provider
  abstraction (CLO-489) does not exist yet and Groq is the only backend, this is fine - but
  add a one-line note that when CLO-489 lands, the fingerprint's provider token must come
  from the active provider, not a literal `"groq:"`, or a cache built under Groq could be
  wrongly reused under another provider for the same repo. (The fingerprint *does* include
  the model, so a model change busts it; but two providers could share a model name.)

## 3. ADR Compliance

Only two ADR files exist (`001-foundational-architecture-decisions.md` + README). I read
ADR-001 in full and checked every decision the design touches:

| ADR-001 Decision | Design compliance |
|---|---|
| #1 Shell out to git (typed wrapper) | Followed - cache reuses `changed_files()`/`commit_signed()`, no new git library. |
| #2 Blocking, no async | Followed - cache is a synchronous in-process module, no runtime. |
| #4 Config dir via `directories`, 0600 secrets | Consistent - cache dir via the same `directories` crate; 0600 on Unix, cache-dir ACL on Windows. Adds `directories` as a dep (already implied by #4/#12). |
| #6 Regenerate-per-group message contract | **Central to the design and correctly applied** - only `groups[0]` carries a message initially; advanced group gets a scoped message-only call; never a grouping call on a hit. The Open Question (zero-LLM-calls reading) is correctly flagged as an ADR-001 change, not a CLO-491 tweak. |
| #12 OS cache dir, drop bash /tmp + FR-30 compat | Followed exactly - new fingerprint envelope is justified precisely because FR-30 bash-read compat was dropped. |

**Violations**: none.
**New ADR needed**: no. The cache file-format version + fingerprint scheme are
implementation detail under #12, not a new architectural decision.

One small consistency note: the design header says it covers FR-30, and the body says FR-30
was *dropped* by ADR-001 #12. Both PROJECT.md and the workflow YAML list FR-30 under
"covers". This is fine (it is "covered" by an explicit decision not to implement bash
compat), but a half-sentence in the Summary saying "FR-30 is covered as an explicit
drop, not an implementation" would avoid a reader thinking it is unaddressed.

## 4. Security Review

- **No secrets in the cache** - the design's Must-not is explicit: only the plan the LLM
  already returned is persisted; no API keys, no raw diffs, no message content beyond the
  plan. Correct. The fingerprint stores only a SHA-256 *digest* of file content, not the
  content itself - good.
- **0600 perms + atomic temp-then-rename** - sound. One gap: the temp file must be created
  *with* 0600 (or chmod'd before the rename), not chmod'd after, to avoid a brief window
  where the temp file is world-readable. The design says "set mode 0600 after an atomic
  write" - tighten to "create the temp file 0600 (or chmod before rename)" so content is
  never momentarily readable. Minor but worth a line given the Security NFR.
- **Cache key is `sha256(repo-root-absolute-path)`** - not secret-bearing, fine. The digest
  of the *path* in the filename leaks nothing sensitive.
- **`gather_for_files` reuses CLO-487's untracked handling**, which already respects
  `--exclude-standard` (gitignored secrets like `.env` are never read - git.rs:124-141 via
  `untracked_files`). The design must ensure `gather_for_files` goes through that same
  gitignore-respecting path and does not naively read group-0 files off disk. Flag this as a
  test (a gitignored file that somehow appears in a group must not be content-hashed or
  diffed). In practice the change set already excludes ignored files, so this is defensive.

No hardcoded secrets, no new egress, no auth surface. Security posture is good.

## 5. Implementation Concerns

- **Phase ordering is right** (deps+module -> helpers -> wiring -> tests). The riskiest
  bullet is the single `gather_for_files` checkbox (see 2-MEDIUM) - split it into
  tracked-diff-scoped + untracked-content + unborn-branch sub-items.
- **`error.rs` change is small but load-bearing** - `CommitFailed` + `leaves_staged()` plus
  the `commit_signed` return-type change (see 2-HIGH). Make the `commit_signed` edit an
  explicit bullet; right now it is implicit.
- **Acceptance harness reuse is realistic** - I read `scripts/acceptance.sh`; the mock-Groq
  server already routes by `"response_format" in body` to distinguish grouping vs message
  calls (the exact signal AC-1 needs: "second run records zero grouping requests, a message
  request is allowed"). The design's AC-1 verification ("the second run records zero
  `response_format`/grouping requests") is directly implementable against the existing mock.
  Good - this is concrete, not hand-wavy.
- **Concurrency**: "last writer wins, atomic rename prevents a torn file" is acceptable for a
  single-user CLI. No locking needed. Agreed.
- **`--dry-run` saves-but-does-not-advance** is a nice touch and directly enables Eval row 2
  (a real run after a dry-run hits with zero LLM calls). Make sure the dry-run *miss* path
  saves the **full** plan (with `groups[0].commit_message` populated) so the follow-up real
  run truly needs zero calls - the design says this; keep the test (Eval #2) load-bearing.

## 6. Blind Spots

- **The `commit_signed` "index restored" lie (2-HIGH)** - the biggest one; the existing
  error text becomes incorrect on the FR-58 path and the discriminator may misfire.
- **Diff source for the message-only call (before vs after staging)** - unstated; pick
  working-tree-before-staging.
- **Untracked-only group on a cache hit** - `gather_for_files` for a group whose files are
  all untracked produces no `git diff`; needs CLO-487-style content read. Not called out.
- **A pending file deleted *and* the plan still references it after an edit between runs** -
  the fingerprint's `\0DELETED` marker handles the *hash* side, but confirm `gather_for_files`
  and `stage_group` already handle a deletion in group 0 (git.rs tests show `stage_group`
  stages deletions, so this is fine - just add a deletion-in-group-0 acceptance case).
- **Stale-cache after a successful commit whose advance-write failed, then the user edits a
  remaining file before the next run** - the design's self-heal argument ("live fingerprint
  != stored -> miss") holds and actually gets *stronger* with an edit; no issue, but worth a
  one-line test (Eval #15 covers the no-edit case).
- **Windows perms** - "cache-dir ACL restricts access" is asserted but not verified. Low
  risk (matches ADR-001 #4's posture), document it as an assumption.
- **What happens if `groups[0].files` on a hit no longer all resolve in the live change
  set** (e.g. the user manually committed one of them out-of-band)? The fingerprint would
  mismatch (the pending set changed) -> miss -> re-analyze. So this is *handled* by the
  freshness check, but only because the fingerprint is over the live pending set - good that
  the design reads pending from live `git status`, not the cached plan. Worth stating as an
  explicit invariant: "a hit guarantees every `groups[0]` file is still pending" follows from
  the fingerprint covering the live set.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is architecturally sound, ADR-compliant, and unusually well-grounded - every
interface it leans on actually exists with the claimed shape, and it correctly fixes the two
bash defects it set out to fix. It is not blocked. The one item that *must* be resolved
before/within implementation is the `commit_signed` -> `CommitFailed` return-type change and
its now-incorrect "index restored" message (2-HIGH); left as-is, AC-3 fails. The other items
are clarifications and sub-step splits, not rework.

## 8. Actionable Feedback (prioritized)

1. **(Must, blocks AC-3)** Make `git.rs::commit_signed` return `GcmError::CommitFailed` on a
   non-zero `git commit` (not `GcmError::Git`), and remove/correct its "index restored"
   message - on the FR-58 path the index is deliberately *not* restored. Add this as an
   explicit Phase-3 bullet, and ensure `clear_staged`/`stage_group` keep returning
   `GcmError::Git` so `leaves_staged()` discriminates without string matching.
2. **(Should)** Pin the message-on-hit diff source: generate the advanced group's message
   from the **working-tree** diff of group 0's files **before** `clear_staged`/`stage_group`,
   mirroring the miss path that has the message in hand before staging.
3. **(Should)** Split the `gather_for_files` Phase-2 item into: (a) path-scoped tracked diff
   (`git diff [HEAD] -- <paths>`, unborn-safe), (b) untracked-file content for group-0
   untracked files (reuse CLO-487's `append_untracked` path, gitignore-respecting), (c) an
   unborn-branch test. This is the largest hidden cost in the plan.
4. **(Should)** Create the cache temp file as 0600 (or chmod before rename), not after, to
   avoid a world-readable window. One line in "Cache location & key".
5. **(Nice)** Add a deletion-in-group-0 acceptance case and an untracked-only-group-0
   acceptance case (both exercise `gather_for_files` + `stage_group` edge paths).
6. **(Nice)** Add a note that the fingerprint's `"groq:"` provider token must become the
   active provider's id once CLO-489 lands (two providers can share a model name).
7. **(Nice)** One sentence in the Summary clarifying FR-30 is "covered" as an explicit drop
   (ADR-001 #12), so it does not read as unaddressed against PROJECT.md/ROADMAP.

---

*This review was produced by a single source-validated reviewer (Claude). Human judgment
should be applied when interpreting these suggestions; the AC-3 item (#1) is the one that
should not be deferred.*
