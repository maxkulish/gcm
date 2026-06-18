# gcm/gcmq/gcmc - LLM Plan Caching

**Added**: 2026-03-02
**Script**: `git/git-commit-ai.sh` (symlinked from `/opt/script/git-commit-ai.sh`)
**Status**: Implemented, committed `19cf39d`, testing in progress

## What it does

`gcm`, `gcmq`, and `gcmc` group changes into logical commits via an LLM call, then
commit one group per run. Before this feature, every subsequent run re-called the LLM
to re-analyze remaining files — wasting tokens and potentially reordering groups.

The cache saves the full grouping plan after the first LLM call. Subsequent runs load
the plan from disk, skipping the LLM call entirely. After each commit, the cache is
updated (committed group removed). When the last group is committed, the cache is
auto-deleted.

## Cache file

- **Location**: `/tmp/gcm-plan-<16-char-sha256-of-repo-root>.json`
- **Content**: JSON with `groups` array (remaining uncommitted groups only)
- **Scope**: per-repo, provider-agnostic (shared across gcm/gcmq/gcmc)
- **Lifetime**: auto-deleted on last commit or by OS temp-file cleanup

## New flag

```
gcm --reset       # delete cache and re-analyze from scratch
gcmq --reset      # same for Groq provider
gcmc --reset      # same for Cerebras provider
```

Use `--reset` when:
- You want a fresh grouping with a different provider
- You manually committed some files outside of `gcm`
- The cached plan no longer reflects your intent

## Cache invalidation logic

Before using the cache, the script compares (sorted):
- Files in the cached plan (all groups combined)
- Files currently in `git status`

If the sets differ (new file added, file committed manually, etc.), the cache is
discarded and the LLM is called fresh. Message: `⚠️  Cache stale, re-analyzing...`

## Edge cases

| Scenario | Behavior |
|---|---|
| `--dry-run` | Uses/saves cache; no cache update (no commit happens) |
| `--all` mode | Clears cache (all files committed in one shot) |
| LLM fallback triggered | Clears cache (all files committed in one shot) |
| `--reset` flag | Deletes cache before starting, always calls LLM |
| Last group committed | Cache auto-deleted after commit |
| New file appears mid-session | Cache invalidated, LLM re-analyzes all remaining |

## Observed behavior (first run, 2026-03-02)

- Single-group session: cache saved → immediately auto-deleted (correct - `groups[1:]` = empty)
- Both script + status file correctly grouped into one logical commit by Haiku
- Commit message generated: `feat(gcm): add provider-agnostic LLM plan caching`
- **Cache lifecycle on single-group sessions**: save → commit → detect empty remaining → delete
  - No leftover `/tmp/gcm-plan-*.json` files for single-group sessions

## Implementation notes

- `_CACHE_KEY` uses `shasum -a 256` on the repo root path (macOS built-in, no extra deps)
- Cache validation uses sorted string equality — O(n log n) subshell sort, negligible vs LLM latency
- `GCM_CACHE_FILE` is a global variable set before `fallback_single_commit()` is ever called,
  so `${GCM_CACHE_FILE:-}` safely handles the case where reset happens before Phase 1
- The `if ! $_USE_CACHED_PLAN; then ... fi` block wraps Phase 2 + Phase 3 + Phase 3b entirely —
  both code paths (cache hit and miss) converge at Phase 4 with identical `JSON_BODY` + `NUM_GROUPS`
- `--dry-run` path exits before the post-commit cache advance, so dry runs don't modify the cache

## Code locations

| Change | Lines (approx) |
|---|---|
| `RESET_CACHE=false` + `--reset` arg | Lines 12, 18 |
| Cache file path + reset logic | Lines 56-62 |
| Cache clear in `fallback_single_commit()` | Line 75 |
| Cache check + conditional LLM block | Lines 215-233 |
| Cache save after successful parse | Line 329 |
| Cache advance after commit | Lines 412-420 |
