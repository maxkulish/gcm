# Design Review Synthesis: CLO-491 - Per-repo plan cache with commit-safe advancement

**Reviewed**: 2026-06-20
**Design Document**: docs/design-docs/2026-06-20-clo-491-plan-cache.md

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini | SKIPPED | External models not invoked in this run |
| Ollama | SKIPPED | External models not invoked in this run |
| Claude (source-validated) | OK | Full review against ADR-001 + the actual src/ tree |

> Single-review synthesis. The one reviewer read the design doc, ADR-001, the project
> aggregation files, and all seven source files the design depends on, validating every
> interface claim against the real code.

## Single Review - Key Findings

### Consolidated Verdict: APPROVE_WITH_SUGGESTIONS

The design is architecturally sound, fully ADR-001 compliant, and unusually well-grounded:
every existing primitive it claims to reuse was verified present with the stated signature.
It correctly fixes the two bash defects it targets (name-only staleness via a content
fingerprint; null-message advancement via the ADR-001 #6 regenerate-per-group contract). Not
blocked. One finding must be resolved during implementation or AC-3 fails.

### Priority Actions

1. **(MUST - blocks AC-3)** `git.rs::commit_signed` currently returns
   `GcmError::Git("...index restored")` on any commit failure (git.rs:161-175). For FR-58
   the commit-failure path must leave the group staged, so `commit_signed` must return the
   new `GcmError::CommitFailed` (and drop the false "index restored" wording). If it keeps
   returning `GcmError::Git`, `leaves_staged()` is `false`, the `main.rs` wrapper restores
   the index, and the group is unstaged - AC-3 fails. Keep `clear_staged`/`stage_group`
   returning `GcmError::Git` so the discriminator works without string matching. The design's
   affected-components table omits this `commit_signed` edit; make it an explicit Phase-3
   bullet.

2. **(SHOULD)** Pin the message-on-cache-hit diff source: generate the advanced group's
   message from the **working-tree** diff of group 0's files **before** staging, mirroring
   the miss path. The design says "scoped to the group's diff" but does not fix before-vs-
   after staging.

3. **(SHOULD)** `gather_for_files` is genuinely new (verified: `diff_full`/`diff_stat` take
   no pathspec) and the plan under-scopes it to one checkbox. Split into: path-scoped tracked
   diff (unborn-safe), untracked-content for untracked group-0 files (reuse CLO-487's
   gitignore-respecting `append_untracked`), and an unborn-branch test. Largest hidden cost.

4. **(SHOULD)** Create the cache temp file as 0600 (or chmod before rename), not after, to
   avoid a brief world-readable window (Security NFR).

5. **(NICE)** Add deletion-in-group-0 and untracked-only-group-0 acceptance cases; note the
   fingerprint's `"groq:"` provider token must follow the active provider once CLO-489 lands;
   clarify in the Summary that FR-30 is "covered" as an explicit drop (ADR-001 #12).

### ADR Compliance

No violations. ADR-001 decisions #1 (shell-out git), #2 (blocking/no-async), #4 (config dir +
0600), #6 (regenerate-per-group), and #12 (OS cache dir, drop /tmp + FR-30 compat) are all
correctly applied. No new ADR needed. The "no new analysis call = no grouping call (message-
only allowed)" reading is correctly flagged for explicit owner sign-off, with the zero-LLM-
calls alternative correctly identified as an ADR-001 change rather than a CLO-491 tweak.

### Security

Clean: no secrets in the cache (only the LLM's plan + content *digests*), gitignore-
respecting reads inherited from CLO-487, atomic temp+rename. Only nit: tighten the temp-file
perms to avoid a world-readable window (action 4).

Full review: `docs/reviews/clo-491-review-claude-fallback.md`.
