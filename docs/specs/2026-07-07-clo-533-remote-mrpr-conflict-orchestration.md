# CLO-533 Add `gcm resolve` remote MR/PR conflict orchestration (Phase 2)

**Status:** draft  
**Type:** specification  
**Linear:** https://linear.app/cloud-ai/issue/CLO-533/add-gcm-resolve-remote-mrpr-conflict-orchestration-phase-2  
**Design context:** `docs/designs/clo-531-gcm-resolve.md` §3 (layered pipeline), §4 (CLI surface), §7 (test plan). Phase 1 core (`src/resolve/`) is reused unchanged.

## 1. Problem and goal

Phase 1 ([CLO-531](https://linear.app/cloud-ai/issue/CLO-531)) shipped a local `gcm resolve` command that works on the current repo's in-progress merge/rebase/cherry-pick conflicts. Phase 2 extends it so a developer can point `gcm resolve` at a GitHub PR or GitLab MR, have `gcm` fetch the source and target branches into an isolated scratch repository, run the merge, and drive the existing Phase-1 resolution engine over the resulting conflicts. Resolutions land on a dedicated branch; the user's checked-out branch and the remote MR/PR branch are never touched unless an explicit opt-in flag is passed.

This is intentionally a thin fetch-then-invoke-core wrapper: all LLM resolution, validation, preview logic, and safety invariants come from the Phase-1 engine. Phase 2 only adds host detection, CLI availability checks, branch isolation, and optional publishing.

## 2. Acceptance criteria

- [ ] **AC1 - CLI surface:** `gcm resolve --pr <url|id>` and `gcm resolve --mr <url|id>` parse and are mutually exclusive. Without either flag the command keeps the existing local Phase-1 behavior.
  - *Verification:* `cargo test cli::resolve_subcommand_parses_with_flags -- --exact`
- [ ] **AC2 - Host auto-detection:** A full GitHub/GitLab URL is parsed into `(host, owner, repo, number)`. A bare numeric id is resolved against the current repo's `origin` remote host. Self-hosted GitHub/GitLab domains are identified by domain heuristics, not only `github.com`/`gitlab.com`.
  - *Verification:* `cargo test resolve_remote::parse_github_url -- --exact`, `cargo test resolve_remote::parse_gitlab_url -- --exact`, and `cargo test resolve_remote::host_from_origin_remote -- --exact`
- [ ] **AC3 - Missing CLI tool is actionable:** When `--pr` is used but `gh` is not on `PATH`, or `--mr` is used but `glab` is not on `PATH`, `gcm` exits with a clear message naming the missing binary and the install/auth hint.
  - *Verification:* `cargo test resolve_remote::missing_gh_error -- --exact` and `cargo test resolve_remote::missing_glab_error -- --exact`
- [ ] **AC4 - Scratch-clone isolation:** The user's working tree is never mutated; source/target branches are fetched into a temporary clone (under `tempfile::TempDir`) with a random path. The resolution branch name remains deterministic (`gcm-resolve-<host>-<number>`), but the scratch directory is always unique.
  - *Verification:* `cargo test resolve_remote::scratch_repo_is_isolated -- --exact`
- [ ] **AC5 - Merge run produces conflicts for the core:** In the scratch repo, `gcm` checks out the base (target) branch, creates a resolution branch, merges the source branch, and invokes the existing `resolve::run_resolve_in_repo` engine.
  - *Verification:* `cargo test resolve_remote::merge_produces_conflicts -- --exact`
- [ ] **AC6 - Dedicated resolution branch:** Resolutions are committed to a branch named `gcm-resolve-<host>-<number>` (e.g. `gcm-resolve-github-123`). It is never the user's current branch and never the MR/PR source branch.
  - *Verification:* `cargo test resolve_remote::resolution_branch_naming -- --exact`
- [ ] **AC7 - Default is local-only:** By default no ref is pushed and no MR/PR comment is posted. The command prints the scratch repo path and the resolution branch name and exits.
  - *Verification:* `cargo test resolve_remote::default_no_push -- --exact`
- [ ] **AC8 - Dry-run purity:** With `--dry-run`, no temporary directory is created, no `git clone`, no `gh`/`glab` checkout, no merge, no resolution branch, no provider write, and no remote mutation occurs. `--remote-push` and `--remote-comment` are ignored under `--dry-run`; output is a preview report only.
  - *Verification:* `cargo test resolve_remote::dry_run_no_clone -- --exact` and `cargo test resolve_remote::dry_run_ignores_remote_flags -- --exact`
- [ ] **AC9 - Optional push and comment:** `--remote-push` pushes the resolution branch to the configured remote; `--remote-comment` posts a concise summary comment on the original PR/MR via `gh pr comment` / `glab mr note`.
  - *Verification:* `cargo test resolve_remote::remote_push_invoked -- --exact` and `cargo test resolve_remote::remote_comment_invoked -- --exact`
- [ ] **AC10 - Partial escalation reporting:** If the Phase-1 core escalates some files to human review, the final report lists the unresolved files and the resolution branch still contains the remaining conflict markers (unless the user edited them).
  - *Verification:* `cargo test resolve_remote::partial_escalation_report -- --exact`
- [ ] **AC11 - Phase-1 core unchanged:** No new resolver logic, marker parser, provider prompt, or validation code is added under `src/resolve/` except orchestration modules (`src/resolve/remote/`). Existing `resolve` unit tests continue to pass.
  - *Verification:* `cargo test resolve:: -- --exact` and `cargo test markers:: -- --exact`
- [ ] **AC12 - Code quality:** `cargo fmt --check` and `cargo clippy -- -D warnings` pass; all new modules have unit tests.
  - *Verification:* `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
- [ ] **AC13 - Scratch repo cleanup:** On every exit path (success, error, or user abort), the scratch directory is removed. No residual `gcm-*` temp directories remain after execution.
  - *Verification:* `cargo test resolve_remote::scratch_cleanup_on_error -- --exact`
- [ ] **AC14 - Clean merge path:** If the source branch merges cleanly into the base, the remote path returns a success status (`resolved` or `noop`) with no provider call and a resolution branch containing the merged tree.
  - *Verification:* `cargo test resolve_remote::clean_merge_no_conflicts -- --exact`

## 3. Sub-tasks

### ST1 Add `--mr` / `--pr` flags to `Commands::Resolve`
**Files:** `src/cli.rs`, `src/cli.rs` tests.  
**Tests:** `resolve_subcommand_parses_with_flags` extended; new `resolve_remote_flags_parses`.  
**Estimate:** S  
Add two optional `Option<String>` arguments (`--mr`, `--pr`) to the existing `Resolve` variant. They are mutually exclusive at parse time via a `clap` `group` or a runtime check. When neither is provided, dispatch to the existing local path.

### ST2 Host detection and CLI availability
**Files:** `src/resolve/remote/host.rs` (new), `src/resolve/remote/mod.rs` (new), `src/error.rs`.  
**Tests:** `parse_github_url`, `parse_gitlab_url`, `host_from_origin_remote`, `missing_gh_error`, `missing_glab_error`, `custom_gitlab_domain`.  
**Estimate:** S  
Define `enum Host { GitHub, GitLab }` with `cli_name()` (`gh`/`glab`), URL patterns, and a parser. Add structured `GcmError` variants:
```rust
RemoteHost { host: String, reason: String }
RemoteCliMissing { cli: String, install_hint: String }
```
Detection uses `git remote get-url origin` for bare ids and domain heuristics for full URLs and self-hosted remotes (e.g. `git@gitlab.company.corp:group/repo.git`).

### ST3 Temp-clone isolation and merge orchestration
**Files:** `src/resolve/remote/fetch.rs` (new), `src/resolve/remote/orchestrate.rs` (new), `src/git.rs` helpers, `src/resolve/mod.rs`.  
**Tests:** `scratch_repo_is_isolated`, `merge_produces_conflicts`, `resolution_branch_naming`, `scratch_cleanup_on_error`, `clean_merge_no_conflicts`.  
**Estimate:** M  
Create a `tempfile::TempDir` scratch clone of the remote repo. Decompose the work:
- **ST3a - Clone:** `git clone <remote-url> <tempdir>`; configure the host CLI credential helper for HTTPS (e.g. `git config credential.helper "!gh auth git-credential"` for GitHub).
- **ST3b - Fetch source branch:** Run `gh pr checkout <id> --branch gcm-resolve-source-<id>` / `glab mr checkout <id> --branch gcm-resolve-source-<id>` in the scratch repo.
- **ST3c - Fetch base branch:** Check out the base (target) branch from the remote.
- **ST3d - Create resolution branch and merge:** Create `gcm-resolve-<host>-<id>` from the base and merge the source branch into it.
- **ST3e - Invoke core engine:** Call `resolve::run_resolve_in_repo(repo, args)`, which accepts a `Repo` and `Cli` and returns `Result<ResolveReport, GcmError>`.

All shell-outs capture stdout/stderr separately and forward diagnostics to `stderr` only, so `--json` stdout remains clean. If the merge produces no conflicts, the engine returns `ResolveStatus::Resolved`/`Noop` instead of `NoConflictInProgress`/`NoConflicts`.

### ST4 Optional publish step (push + comment)
**Files:** `src/resolve/remote/publish.rs` (new), `src/resolve/remote/mod.rs`, `src/resolve/report.rs`.  
**Tests:** `remote_push_invoked`, `remote_comment_invoked`, `default_no_push`.  
**Estimate:** M  
If `--remote-push` is passed, run `git push -u <remote> <resolution-branch>`. If `--remote-comment` is passed, run `gh pr comment <id> --body-file <tmp>` / `glab mr note <id> --message <tmp>` with a short summary (file count, escalated count). Both are off by default.

### ST5 Wire remote path and enrich the report
**Files:** `src/resolve/mod.rs`, `src/resolve/report.rs`, `src/main.rs`.  
**Tests:** `partial_escalation_report`, `dry_run_no_clone`, `remote_report_json_shape`, `clean_merge_no_conflicts`.  
**Estimate:** M  
Refactor `resolve::run_resolve` into `resolve::run_resolve_in_repo(repo: &Repo, args: &Cli) -> Result<ResolveReport, GcmError>` so the local path and remote path share the same core. The local path discovers the repo then calls `run_resolve_in_repo`; the remote path calls it on the scratch repo. Extend `ResolveReport` with an optional `remote: RemoteReport` block:

```rust
pub struct RemoteReport {
    pub host: Host,
    pub number: u64,
    pub base_branch: String,
    pub source_branch: String,
    pub resolution_branch: String,
    pub pushed: bool,
    pub commented: bool,
}
```

Emit it on `--json`.

### ST6 Integration/acceptance tests with fixture scripts
**Files:** `tests/resolve_remote_integration.rs`, `tests/fixtures/fake-gh`, `tests/fixtures/fake-glab`.  
**Tests:** host-scenario table (see §4).  
**Estimate:** L  
Build shell-script mocks for `gh`/`glab` and `git` helpers that return JSON/refs as needed. Assert command-line invocations, branch names, and dry-run behavior. Real network calls are never made in tests.

## 4. Evaluation table

| # | Scenario | Input | Expected | Verification |
|---|---|---|---|---|
| 1 | GitHub PR URL | `gcm resolve --pr https://github.com/acme/app/pull/42 --dry-run` | Host=GitHub, owner=acme, repo=app, number=42; no clone | `cargo test resolve_remote::parse_github_url` |
| 2 | GitLab MR URL | `gcm resolve --mr https://gitlab.com/acme/app/-/merge_requests/42 --dry-run` | Host=GitLab, owner=acme, repo=app, number=42 | `cargo test resolve_remote::parse_gitlab_url` |
| 3 | Self-hosted GitLab origin | Inside a repo whose origin is `git@gitlab.company.corp:acme/app.git`, run `gcm resolve --mr 42 --dry-run` | Host=GitLab, number=42 | `cargo test resolve_remote::host_from_origin_remote` |
| 4 | Missing `gh` | `gcm resolve --pr 1` with `gh` removed from PATH | `GcmError::RemoteCliMissing` naming `gh` and install/auth hint | `cargo test resolve_remote::missing_gh_error` |
| 5 | Missing `glab` | `gcm resolve --mr 1` with `glab` removed from PATH | `GcmError::RemoteCliMissing` naming `glab` and install/auth hint | `cargo test resolve_remote::missing_glab_error` |
| 6 | Scratch isolation | Run remote resolve in a temp repo | Scratch clone path is outside the user's repo root; user repo unchanged | `cargo test resolve_remote::scratch_repo_is_isolated` |
| 7 | Merge + resolution branch | Mock `gh` returns head=feature, base=main | Scratch repo has branch `gcm-resolve-github-42` containing resolved content | `cargo test resolve_remote::resolution_branch_naming` |
| 8 | Dry-run purity | `gcm resolve --pr ... --dry-run` | No `git clone`, no `gh pr checkout`, no provider call, no push | `cargo test resolve_remote::dry_run_no_clone` |
| 9 | Dry-run ignores remote flags | `gcm resolve --pr ... --dry-run --remote-push --remote-comment` | No push, no comment, no temp dir created | `cargo test resolve_remote::dry_run_ignores_remote_flags` |
| 10 | Default no push | Resolve without `--remote-push` | `git push` is not invoked; report.pushed=false | `cargo test resolve_remote::default_no_push` |
| 11 | Opt-in push | `gcm resolve --pr ... --remote-push` | `git push -u origin gcm-resolve-github-42` invoked | `cargo test resolve_remote::remote_push_invoked` |
| 12 | Opt-in comment | `gcm resolve --pr ... --remote-comment` | `gh pr comment 42 --body-file ...` invoked | `cargo test resolve_remote::remote_comment_invoked` |
| 13 | Clean merge | Source branch merges cleanly into base | Status resolved/noop, no LLM call, no conflicts in resolution branch | `cargo test resolve_remote::clean_merge_no_conflicts` |
| 14 | Partial escalation | One file auto-resolved, one escalated | Report lists escalated files; branch keeps markers for escalated files | `cargo test resolve_remote::partial_escalation_report` |

## 5. Edge cases

- **EC1 - Bare id with no `origin` remote:** The user passes `--pr 42` outside a repo or in a repo with no origin. → `GcmError::RemoteHost` asking for a full URL.
- **EC2 - Unsupported host in URL:** A URL from `bitbucket.org` or a self-hosted instance whose domain is ambiguous. → `GcmError::RemoteHost` listing supported host families (GitHub-like / GitLab-like) and suggesting a full URL with a recognizable host.
- **EC3 - Clean merge (no conflicts):** The source branch merges cleanly into the base. → The report status is `resolved` or `noop`, no provider call, resolution branch simply contains the merged tree.
- **EC4 - Source branch not reachable / auth failure:** `gh pr checkout` or `glab mr checkout` fails. → Surface the host CLI's stderr as a `GcmError::RemoteHost` or `GcmError::Git` message and clean up the scratch repo.
- **EC5 - User's current repo is the same as the remote repo:** Even if the local repo is the same, a scratch clone is still used so the user's index and working tree are untouched.
- **EC6 - `--remote-push` without network or without write permission:** The `git push` failure is surfaced; the local resolution branch is left in the scratch repo so the user can push manually.
- **EC7 - `--remote-comment` on a closed/merged MR/PR:** Host CLI returns an error; surface it but do not abort the local resolution.
- **EC8 - Binary files in the remote MR/PR:** The Phase-1 engine already skips binary conflicted files; the remote report lists them as escalated, same as local.
- **EC9 - `--yes` with escalations:** Escalated files remain conflicted in the resolution branch regardless of `--yes`; only validated resolutions are written.
- **EC10 - `--secret-scan=abort` with a credential in a conflict hunk:** The Phase-1 engine aborts before provider egress; remote wrapper reports `error` and cleans up the scratch repo.
- **EC11 - Resolution branch collision:** If `gcm-resolve-github-123` already exists in the scratch repo, the new run overwrites it with `git checkout -B` from the base (a fresh scratch clone should not contain it unless a previous run left state; this is the deterministic behavior).

## 6. Notes / constraints

- No new token store is introduced; authentication is inherited from the user's existing `gh`/`glab` login state.
- No native GitHub/GitLab API client crate is added. `gh` and `glab` are treated as external binaries on `PATH`, matching the `git`/`mergiraf` pattern.
- The Phase-1 `[conflict]` config block, `--dry-run`, `--json`, `--yes`, `.gcmignore`, and `--secret-scan` are honored unchanged because the remote path reuses `resolve::run_resolve_in_repo` with the same `Cli` args.
- Branch creation, merge, and optional push happen only in the scratch repo; the user's local branch is never checked out or modified.
- New orchestration errors extend the central `GcmError` enum with structured fields:
  ```rust
  RemoteHost { host: String, reason: String }
  RemoteCliMissing { cli: String, install_hint: String }
  ```
- The remote report shape is defined as:
  ```rust
  pub struct RemoteReport {
      pub host: Host,
      pub number: u64,
      pub base_branch: String,
      pub source_branch: String,
      pub resolution_branch: String,
      pub pushed: bool,
      pub commented: bool,
  }
  ```
- Self-hosted GitHub/GitLab instances are detected by domain heuristics (e.g. a host matching `*github*` or `*gitlab*`, or via user-provided full URL) rather than a hard-coded allow-list.
- All shell-outs to `gh`/`glab`/`git` capture stdout/stderr separately and forward errors to `stderr` only, preserving the single-JSON-object stdout contract.
- Long-running shell-outs (`gh pr checkout`, `git clone`, `git push`) are wrapped with a bounded timeout to avoid indefinite hangs.
- The scratch clone configures the host CLI credential helper so HTTPS fetch/push reuses the user's `gh`/`glab` auth (e.g. `git config credential.helper "!gh auth git-credential"`).
