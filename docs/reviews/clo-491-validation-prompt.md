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
