# Pre-PR validation: clo-533

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-07
**Pipeline**: lok pre-pr-validation
---

I have verified every decisive claim against the actual code. Key confirmations:

- **`has_conflict_state()`** = `MERGE_HEAD || REBASE_HEAD || CHERRY_PICK_HEAD` (git.rs:292). A clean `merge --no-ff --no-commit` sets `MERGE_HEAD`, so `has_conflict_state()` is true, the `allow_no_conflict_state` guard is skipped, `unmerged_files()` is empty, and `run_resolve_in_repo` returns `Err(NoConflicts)`. Codex's clean-merge HIGH is a true positive; Gemini missed it.
- **No commit anywhere**: `run_resolve_in_repo` only `write_file`s to the working tree; `mod.rs` never runs `git add`/`git commit`. The resolution branch stays at the base commit and the `TempDir` is dropped. Confirmed.
- **`format_origin_url`** hardcodes `github.com`/`gitlab.com` and `RemoteRef` has no domain field, so self-hosted clones the wrong public repo. Confirmed.
- The `git diff --check` failures are markdown two-space hard-line-breaks in docs only.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Verdict FAIL. Core findings (never-committed, clean-merge, self-hosted, default reporting) all verified against source. |
| Gemini | OK (under-called) | Verdict PASS_WITH_NOTES. Caught tests + timeouts but missed the CRITICAL never-committed bug, the clean-merge break, and the self-hosted clone-URL break; its "domain heuristic robust" note validated detection only, not the clone path. |
| Claude fallback | SKIPPED | At least one external reviewer succeeded, per protocol. |

## Verdict
FAIL

## Must Fix Before PR
- **Remote resolutions are never staged or committed (CRITICAL, confirmed).** `run_resolve_in_repo` writes resolved content to the scratch working tree only; `mod.rs` does no `git add`/`git commit`, so the `gcm-resolve-<host>-<number>` branch never moves off the base commit and the `TempDir` (fetch.rs:26) is dropped on return. Default mode loses the work; `--remote-push` (fetch.rs:206) pushes the base commit, not the resolved tree. Breaks AC6, AC7, AC9, AC10, AC13, AC14 - the feature's central deliverable is absent.
- **Clean merge returns `NoConflicts` error (HIGH, confirmed).** Because `--no-commit` leaves `MERGE_HEAD` set, `has_conflict_state()` (git.rs:292) is true, so `run_resolve_in_repo` (resolve/mod.rs:67-88) bypasses the `allow_no_conflict_state` success path and falls through to `Err(NoConflicts)`. AC14 is inverted: a clean merge fails instead of succeeding.
- **Self-hosted host resolves to the wrong repo; unsupported hosts silently accepted (HIGH, confirmed).** `RemoteRef` stores no domain (host.rs:47-53) and `format_origin_url` hardcodes `github.com`/`gitlab.com` (fetch.rs:81-92), so `gitlab.company.corp/acme/app` clones `gitlab.com/acme/app`. Separately, `detect_host` returns the preferred host for any unknown URL when `--pr`/`--mr` is set (host.rs:209-211), so a `bitbucket.org` URL is accepted instead of erroring. Breaks AC2/EC2. (Gemini's "robust" LOW note only checked family detection, not the clone URL.)
- **Default remote runs emit no metadata (HIGH, confirmed).** Human mode prints nothing for the remote path (main.rs:113-129), and `report.remote` is populated only inside the push/comment branch (remote/mod.rs:86-95), so a default run omits the scratch path and resolution branch entirely. Breaks AC7.
- **Comment failure aborts the whole resolution (MEDIUM, confirmed).** `publish(...)?` (remote/mod.rs:78) propagates a `gh pr comment` / `glab mr note` error as a command failure, contrary to EC7 (surface but do not abort).
- **Acceptance-test suite is ~2 of 14 (HIGH, both reviewers).** Only `parse_github_url` and `dry_run_no_clone` exist in tests/resolve_remote.rs; the entire §4 evaluation table (missing-CLI, isolation, branch naming, default-no-push, push, comment, partial escalation, cleanup, clean merge) is unverified. This gap is why the CRITICAL, clean-merge, and reporting bugs survived - no test asserts the resolution branch actually contains a resolved commit.

## Out of Scope / Deferred
- **Subprocess timeouts ignored (`let _ = timeout;` at fetch.rs:244/265, publish.rs:99).** Both reviewers flagged it; the author deliberately deferred, citing ADR-001's synchronous model in an inline comment. Real gap vs spec §6, but a design-note item rather than core correctness. Resolve by either implementing bounded timeouts or amending the spec §6 constraint - not the blocker.
- **Dry-run over-strictness.** `run_resolve_subcommand` calls `Repo::discover()` (main.rs:83) and `require_host_cli` runs before the dry-run short-circuit (remote/mod.rs:36-41), so `--pr <full-url> --dry-run` still needs a repo and `gh`/`glab` installed. Stricter than AC8's "preview only" intent, but the shipped test passes and it is trivially fixable inside the rework pass. Fold in if convenient.

## False Positives / Tooling Artifacts
- **Codex: `git diff --check main...HEAD` failure.** Verified - every hit is trailing whitespace in `docs/**.md` where two trailing spaces are legitimate GitHub-flavored-markdown hard line breaks. Not a code defect; non-blocking.
- **Gemini finding #3 ("domain heuristic robust", LOW).** Misleading reassurance: detection returns the correct host family, but the downstream clone URL drops the domain (see Must Fix #3). Treat as evidence of the self-hosted bug, not a positive.
- **fmt/clippy pass** (both reviewers, `#[allow(dead_code)]` annotations present): accepted as true; not a concern.

## Recommendation
Do NOT transition to PR - route back to the implementation phase for a rework iteration. This is not a product pivot: the fixes are known and the design intent is sound (the code comments show the author meant to commit and to treat a clean merge as success). But the branch does not deliver its core artifact - it never commits the resolved merge, so the resolution is lost on every path - and that sits alongside an inverted clean-merge path, a self-hosted clone that targets the wrong repo, missing default-mode reporting, an EC7 abort, and a near-absent acceptance suite (2 of 14). That is rework, not a single bounded patch, which is why the verdict is FAIL rather than PASS_WITH_NOTES. The rework pass must: (1) after `run_resolve_in_repo` succeeds, `git add` the resolved tree and create the merge commit on `gcm-resolve-<host>-<number>` before any push or cleanup, handling the partial-escalation case (AC10) where escalated files retain markers; (2) make the clean-merge path return `Resolved`/`Noop` with a committed merge; (3) preserve the real remote domain in `RemoteRef` and reject unsupported full URLs even when `--pr`/`--mr` is set; (4) always attach `RemoteReport` and print the branch/path in human mode; (5) not abort on comment failure (EC7); (6) add the named integration tests so each AC is actually exercised. Surface two scope questions to the user before re-review: whether real subprocess timeouts are required now or the spec §6 note should be amended per ADR-001, and whether self-hosted GitHub/GitLab must be end-to-end functional in this PR.
