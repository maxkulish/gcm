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
session id: 019ee68f-3f27-7940-8091-f31b968a4a56
--------
user
Re-validate CLO-491 after fixes (read-only; do not modify files). The prior review (FAIL) raised two findings; confirm they are resolved and look for any NEW issues introduced by the fixes.

1. HIGH content_hash followed symlinks. Check src/cache.rs hash_path()/content_hash(): it must use symlink_metadata (no follow), hash a symlink by its target path, mark special (FIFO/device) files without opening them, and stream regular files. Verify the 3 new unit tests (hash_path_*) actually prove this.
2. MEDIUM --all --dry-run did not clear the cache. This is now DOCUMENTED as intentional (a dry-run must mutate nothing, FR-7; real --all/fallback still clear, FR-28). Confirm src/main.rs single_commit reflects this and the AC-C-drynoclear test locks it. State whether you accept this resolution.

Also confirm the added acceptance cases (rename eval4, hook reformat+restage eval6, untracked-only eval18, deletion-only eval17, fallback-clears eval10, AC-C3 byte-identical) are correct and meaningful.

Read: git diff f021691..HEAD, src/cache.rs, src/main.rs, scripts/acceptance.sh.

OUTPUT:
## Verdict: [PASS | PASS_WITH_NOTES | FAIL]
## Resolved
## New Issues
## Notes
codex
I’m validating the two prior findings against the actual fixes and then checking the touched acceptance coverage for regressions or gaps. First step is to read the diff and the specific files you named, then I’ll assess whether the new tests prove the intended behavior.
exec
/bin/zsh -lc 'git diff --stat f021691..HEAD' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
exec
/bin/zsh -lc "sed -n '1,260p' src/cache.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
exec
/bin/zsh -lc "sed -n '1,260p' src/main.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
 scripts/acceptance.sh | 100 +++++++++++++++++++++++++++++++++++++++++++++++++-
 src/cache.rs          |  88 +++++++++++++++++++++++++++++++++++++++++---
 src/main.rs           |   6 ++-
 3 files changed, 186 insertions(+), 8 deletions(-)

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
    // plan (FR-28) - but only on the REAL (non-dry-run) path. A `--dry-run`
    // (incl. `--all --dry-run` and a dry-run fallback) returns above and clears
    // nothing: a preview must mutate no state (FR-7). A stale cache left behind
    // by a dry-run is harmless - the next real run re-validates the fingerprint
    // and re-analyzes on a mismatch.
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
/// chunks so a large binary still in `git status` cannot OOM the process.
///
/// Symlinks and special files are handled WITHOUT following them (mirrors
/// `diff::append_untracked`): `symlink_metadata` does not traverse the link, so
/// we never read into a FIFO/device/socket (which could block forever) or a
/// symlink target outside the repo (a content leak). A symlink is hashed by its
/// **target path** - exactly the blob git records for it - not the pointed-to
/// bytes. Each non-regular kind and the deleted/unreadable cases get a distinct
/// `\0`-prefixed marker (a real content hash is hex, so they never collide).
fn content_hash(repo: &Repo, file: &ChangedFile) -> String {
    hash_path(&repo.root().join(&file.path))
}

/// Kind-aware content hash of a single path (the body of [`content_hash`],
/// split out so the symlink/special-file safety is unit-testable without a git
/// repo).
fn hash_path(full: &Path) -> String {
    let meta = match fs::symlink_metadata(full) {
        Ok(m) => m,
        Err(_) => return "\0DELETED".to_string(),
    };
    let ft = meta.file_type();
    if ft.is_symlink() {
        return match fs::read_link(full) {
            Ok(target) => {
                let mut h = Sha256::new();
                h.update(b"\0SYMLINK\0");
                h.update(target.to_string_lossy().as_bytes());
                hex(&h.finalize())
            }
            Err(_) => "\0UNREADABLE".to_string(),
        };
    }
    if !ft.is_file() {
        // FIFO/device/socket: never opened/read (would block); a stable marker.
        return "\0SPECIAL".to_string();
    }
    let f = match fs::File::open(full) {
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


exec
/bin/zsh -lc 'git diff --unified=80 f021691..HEAD -- src/cache.rs src/main.rs scripts/acceptance.sh' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
diff --git a/scripts/acceptance.sh b/scripts/acceptance.sh
index af6dd4f..dbfcb00 100755
--- a/scripts/acceptance.sh
+++ b/scripts/acceptance.sh
@@ -460,221 +460,317 @@ if [ "$SIGNING_OK" -eq 1 ]; then
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
-cf="$(cache_file)"
-[ -n "$cf" ] && grep -q '"src.txt"' "$cf" && ok "cache un-advanced (still holds group 1)" || bad "cache advanced despite the commit failure"
+cf="$(cache_file)"; before="$(cat "$cf" 2>/dev/null)"
+{ [ -n "$before" ] && printf '%s' "$before" | grep -q '"src.txt"' && printf '%s' "$before" | grep -q '"docs.md"'; } && ok "cache un-advanced (still the full plan: both groups)" || bad "cache not the full un-advanced plan"
 [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "no commit created" || bad "a commit slipped through the rejecting hook"
+# A second rejected run must not mutate the cache (idempotent; never advances).
+( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
+[ "$before" = "$(cat "$cf" 2>/dev/null)" ] && ok "cache byte-identical after a repeated rejected commit" || bad "cache changed across rejected retries"
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
 
+note "AC-C-rename: renaming a pending file invalidates the cache (eval 4)"
+if [ "$SIGNING_OK" -eq 1 ]; then
+  reset_cache; d="$(cache_repo_2group)"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
+  ( cd "$d" && git mv docs.md docs2.md )   # rename the still-pending group-2 file
+  printf '%s' '{"groups":[{"files":["docs2.md"],"summary":"docs","commit_message":"docs: renamed"}]}' > "$PLAN_FILE"
+  : > "$CAPTURE"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
+  grep -q '"response_format"' "$CAPTURE" && ok "rename invalidated the cache -> grouping call" || bad "stale cache reused after a rename"
+  reset_cache; rm -rf "$d"
+else
+  skip "AC-C-rename needs signing"
+fi
+
+note "AC-C-hookfix: a hook that reformats+restages lets the commit succeed and the cache advance (eval 6)"
+if [ "$SIGNING_OK" -eq 1 ]; then
+  reset_cache; d="$(cache_repo_2group)"
+  mkdir -p "$d/.git/hooks"
+  printf '#!/bin/sh\nprintf "reformatted\\n" > src.txt\ngit add src.txt\nexit 0\n' > "$d/.git/hooks/pre-commit"
+  chmod +x "$d/.git/hooks/pre-commit"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
+  [ $rc -eq 0 ] && ok "hook reformat+restage -> commit succeeds" || bad "reformatting hook run (rc=$rc; $(tail -1 /tmp/gcm-out))"
+  git -C "$d" show HEAD:src.txt | grep -q 'reformatted' && ok "committed the hook's reformatted content" || bad "reformatted content not committed"
+  cf="$(cache_file)"
+  [ -n "$cf" ] && grep -q '"docs.md"' "$cf" && ok "cache advanced to group 2 after the hook-fixed commit" || bad "cache did not advance after a successful commit"
+  reset_cache; rm -rf "$d"
+else
+  skip "AC-C-hookfix needs signing"
+fi
+
+note "AC-C-untracked: an untracked-only cached group commits on the next run (eval 18)"
+if [ "$SIGNING_OK" -eq 1 ]; then
+  reset_cache; d="$(new_repo)"
+  printf 'seed\n' > "$d/seed.txt"; git -C "$d" -c commit.gpgsign=false add -A >/dev/null
+  git -C "$d" -c commit.gpgsign=false commit -qm init
+  printf 'A\n' > "$d/g1.txt"; printf 'B\n' > "$d/g2.txt"   # both untracked
+  printf '%s' '{"groups":[{"files":["g1.txt"],"summary":"g1","commit_message":"feat: g1"},{"files":["g2.txt"],"summary":"g2","commit_message":null}]}' > "$PLAN_FILE"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
+  : > "$CAPTURE"; : > "$PLAN_FILE"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
+  [ $rc -eq 0 ] && ok "untracked cached group commit exit 0" || bad "untracked cached group (rc=$rc; $(tail -1 /tmp/gcm-out))"
+  grep -q '"response_format"' "$CAPTURE" && bad "made a grouping call (cache missed)" || ok "no grouping call (cache hit on the untracked group)"
+  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'g2.txt' && ok "untracked group 2 committed from cache" || bad "g2.txt not committed"
+  reset_cache; rm -rf "$d"
+else
+  skip "AC-C-untracked needs signing"
+fi
+
+note "AC-C-delete: a deletion-only cached group commits the removal (eval 17)"
+if [ "$SIGNING_OK" -eq 1 ]; then
+  reset_cache; d="$(new_repo)"
+  printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
+  git -C "$d" -c commit.gpgsign=false add -A >/dev/null; git -C "$d" -c commit.gpgsign=false commit -qm init
+  printf 'a2\n' > "$d/a.txt"; rm "$d/b.txt"   # group 1 modifies a.txt, group 2 deletes b.txt
+  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"},{"files":["b.txt"],"summary":"rm b","commit_message":null}]}' > "$PLAN_FILE"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
+  : > "$CAPTURE"; : > "$PLAN_FILE"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
+  [ $rc -eq 0 ] && ok "deletion cached group commit exit 0" || bad "deletion cached group (rc=$rc; $(tail -1 /tmp/gcm-out))"
+  git -C "$d" ls-files | grep -qx 'b.txt' && bad "b.txt still tracked (deletion not committed)" || ok "b.txt deletion committed from cache"
+  reset_cache; rm -rf "$d"
+else
+  skip "AC-C-delete needs signing"
+fi
+
+note "AC-C-fallback: a grouping fallback clears the cache (eval 10 fallback half)"
+if [ "$SIGNING_OK" -eq 1 ]; then
+  reset_cache; d="$(cache_repo_2group)"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
+  [ -n "$(cache_file)" ] && ok "cache warmed before fallback" || bad "no cache to clear"
+  printf 'edited\n' > "$d/docs.md"               # invalidate -> next run is a miss
+  printf '%s' '{ not valid json' > "$PLAN_FILE"   # grouping returns malformed -> fallback
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
+  grep -qi "Falling back" /tmp/gcm-out && ok "malformed plan -> fallback" || bad "no fallback on malformed plan"
+  [ -z "$(cache_file)" ] && ok "fallback cleared the cache" || bad "fallback left the cache in place"
+  reset_cache; rm -rf "$d"
+else
+  skip "AC-C-fallback needs signing"
+fi
+
+note "AC-C-drynoclear: --all --dry-run previews without clearing the cache (FR-7 no-mutation)"
+if [ "$SIGNING_OK" -eq 1 ]; then
+  reset_cache; d="$(cache_repo_2group)"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
+  before="$(cat "$(cache_file)" 2>/dev/null)"
+  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --dry-run >/tmp/gcm-out 2>&1 )
+  after="$(cat "$(cache_file)" 2>/dev/null)"
+  { [ -n "$before" ] && [ "$before" = "$after" ]; } && ok "--all --dry-run left the cache untouched" || bad "--all --dry-run mutated the cache"
+  reset_cache; rm -rf "$d"
+else
+  skip "AC-C-drynoclear needs signing"
+fi
+
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
diff --git a/src/cache.rs b/src/cache.rs
index 6429980..98f40d2 100644
--- a/src/cache.rs
+++ b/src/cache.rs
@@ -117,166 +117,198 @@ fn read_cache_file(bytes: &[u8]) -> Option<CacheFile> {
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
-/// chunks so a large binary still in `git status` cannot OOM the process. A
-/// pending deletion (file absent) is a distinct marker so a delete is detected;
-/// an unreadable file is its own marker (never silently equal to a real hash).
+/// chunks so a large binary still in `git status` cannot OOM the process.
+///
+/// Symlinks and special files are handled WITHOUT following them (mirrors
+/// `diff::append_untracked`): `symlink_metadata` does not traverse the link, so
+/// we never read into a FIFO/device/socket (which could block forever) or a
+/// symlink target outside the repo (a content leak). A symlink is hashed by its
+/// **target path** - exactly the blob git records for it - not the pointed-to
+/// bytes. Each non-regular kind and the deleted/unreadable cases get a distinct
+/// `\0`-prefixed marker (a real content hash is hex, so they never collide).
 fn content_hash(repo: &Repo, file: &ChangedFile) -> String {
-    let full = repo.root().join(&file.path);
-    let f = match fs::File::open(&full) {
+    hash_path(&repo.root().join(&file.path))
+}
+
+/// Kind-aware content hash of a single path (the body of [`content_hash`],
+/// split out so the symlink/special-file safety is unit-testable without a git
+/// repo).
+fn hash_path(full: &Path) -> String {
+    let meta = match fs::symlink_metadata(full) {
+        Ok(m) => m,
+        Err(_) => return "\0DELETED".to_string(),
+    };
+    let ft = meta.file_type();
+    if ft.is_symlink() {
+        return match fs::read_link(full) {
+            Ok(target) => {
+                let mut h = Sha256::new();
+                h.update(b"\0SYMLINK\0");
+                h.update(target.to_string_lossy().as_bytes());
+                hex(&h.finalize())
+            }
+            Err(_) => "\0UNREADABLE".to_string(),
+        };
+    }
+    if !ft.is_file() {
+        // FIFO/device/socket: never opened/read (would block); a stable marker.
+        return "\0SPECIAL".to_string();
+    }
+    let f = match fs::File::open(full) {
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
 
@@ -289,145 +321,191 @@ mod tests {
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
 
+    #[test]
+    fn hash_path_hashes_regular_file_content() {
+        let dir = tempfile::tempdir().unwrap();
+        let p = dir.path().join("f.txt");
+        std::fs::write(&p, b"hello").unwrap();
+        let h = hash_path(&p);
+        assert_eq!(h.len(), 64, "regular file -> hex sha256");
+        let p2 = dir.path().join("g.txt");
+        std::fs::write(&p2, b"hello").unwrap();
+        assert_eq!(h, hash_path(&p2), "same content -> same hash");
+        std::fs::write(&p2, b"world").unwrap();
+        assert_ne!(h, hash_path(&p2), "different content -> different hash");
+    }
+
+    #[test]
+    fn hash_path_missing_file_is_deleted_marker() {
+        let dir = tempfile::tempdir().unwrap();
+        assert_eq!(hash_path(&dir.path().join("nope")), "\0DELETED");
+    }
+
+    #[cfg(unix)]
+    #[test]
+    fn hash_path_does_not_follow_symlinks() {
+        use std::os::unix::fs::symlink;
+        let dir = tempfile::tempdir().unwrap();
+        let target = dir.path().join("target.txt");
+        std::fs::write(&target, b"secret-bytes").unwrap();
+        let link = dir.path().join("link.txt");
+        symlink(&target, &link).unwrap();
+        // A symlink is hashed by its target PATH, not the pointed-to bytes - so
+        // it must differ from the regular-file hash of that content (proving the
+        // link was not followed: no FIFO-block, no out-of-repo content leak).
+        assert_ne!(
+            hash_path(&link),
+            hash_path(&target),
+            "symlink must not be followed into its target's content"
+        );
+        let link2 = dir.path().join("link2.txt");
+        symlink(&target, &link2).unwrap();
+        assert_eq!(
+            hash_path(&link),
+            hash_path(&link2),
+            "same target path -> same symlink hash"
+        );
+    }
+
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
diff --git a/src/main.rs b/src/main.rs
index ff796ba..6ec44a0 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -130,146 +130,150 @@ fn build_plan(repo: &Repo, changed: &[ChangedFile]) -> Result<Plan, BuildError>
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
-    // plan (FR-28); reached only on the real (non-dry-run) path.
+    // plan (FR-28) - but only on the REAL (non-dry-run) path. A `--dry-run`
+    // (incl. `--all --dry-run` and a dry-run fallback) returns above and clears
+    // nothing: a preview must mutate no state (FR-7). A stale cache left behind
+    // by a dry-run is harmless - the next real run re-validates the fingerprint
+    // and re-analyzes on a mismatch.
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

codex
I have the changed code in view now. Next I’m checking whether the fixes are correct in behavior, and separately whether the added tests actually lock those behaviors rather than only exercising happy paths.
exec
/bin/zsh -lc "nl -ba src/cache.rs | sed -n '150,260p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
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
   197	/// chunks so a large binary still in `git status` cannot OOM the process.
   198	///
   199	/// Symlinks and special files are handled WITHOUT following them (mirrors
   200	/// `diff::append_untracked`): `symlink_metadata` does not traverse the link, so
   201	/// we never read into a FIFO/device/socket (which could block forever) or a
   202	/// symlink target outside the repo (a content leak). A symlink is hashed by its
   203	/// **target path** - exactly the blob git records for it - not the pointed-to
   204	/// bytes. Each non-regular kind and the deleted/unreadable cases get a distinct
   205	/// `\0`-prefixed marker (a real content hash is hex, so they never collide).
   206	fn content_hash(repo: &Repo, file: &ChangedFile) -> String {
   207	    hash_path(&repo.root().join(&file.path))
   208	}
   209	
   210	/// Kind-aware content hash of a single path (the body of [`content_hash`],
   211	/// split out so the symlink/special-file safety is unit-testable without a git
   212	/// repo).
   213	fn hash_path(full: &Path) -> String {
   214	    let meta = match fs::symlink_metadata(full) {
   215	        Ok(m) => m,
   216	        Err(_) => return "\0DELETED".to_string(),
   217	    };
   218	    let ft = meta.file_type();
   219	    if ft.is_symlink() {
   220	        return match fs::read_link(full) {
   221	            Ok(target) => {
   222	                let mut h = Sha256::new();
   223	                h.update(b"\0SYMLINK\0");
   224	                h.update(target.to_string_lossy().as_bytes());
   225	                hex(&h.finalize())
   226	            }
   227	            Err(_) => "\0UNREADABLE".to_string(),
   228	        };
   229	    }
   230	    if !ft.is_file() {
   231	        // FIFO/device/socket: never opened/read (would block); a stable marker.
   232	        return "\0SPECIAL".to_string();
   233	    }
   234	    let f = match fs::File::open(full) {
   235	        Ok(f) => f,
   236	        Err(_) => return "\0DELETED".to_string(),
   237	    };
   238	    let mut hasher = Sha256::new();
   239	    let mut reader = BufReader::new(f);
   240	    let mut buf = [0u8; 64 * 1024];
   241	    loop {
   242	        match reader.read(&mut buf) {
   243	            Ok(0) => break,
   244	            Ok(n) => hasher.update(&buf[..n]),
   245	            Err(_) => return "\0UNREADABLE".to_string(),
   246	        }
   247	    }
   248	    hex(&hasher.finalize())
   249	}
   250	
   251	/// Lowercase hex encoding (avoids pulling in the `hex` crate).
   252	fn hex(bytes: &[u8]) -> String {
   253	    use std::fmt::Write as _;
   254	    let mut s = String::with_capacity(bytes.len() * 2);
   255	    for b in bytes {
   256	        let _ = write!(s, "{b:02x}");
   257	    }
   258	    s
   259	}
   260	

exec
/bin/zsh -lc "nl -ba src/cache.rs | sed -n '320,410p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
exec
/bin/zsh -lc "nl -ba src/main.rs | sed -n '170,225p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   320	        }
   321	    }
   322	
   323	    fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
   324	        pairs
   325	            .iter()
   326	            .map(|(p, h)| (p.to_string(), h.to_string()))
   327	            .collect()
   328	    }
   329	
   330	    #[test]
   331	    fn hex_encodes_lowercase_padded() {
   332	        assert_eq!(hex(&[0x00, 0x0f, 0xff, 0xa0]), "000fffa0");
   333	    }
   334	
   335	    #[test]
   336	    fn repo_key_is_stable_and_path_specific() {
   337	        let a = repo_key(Path::new("/home/u/repo"));
   338	        let b = repo_key(Path::new("/home/u/repo"));
   339	        let c = repo_key(Path::new("/home/u/other"));
   340	        assert_eq!(a, b, "same path -> same key");
   341	        assert_ne!(a, c, "different path -> different key");
   342	        assert_eq!(a.len(), 64, "full sha256 hex");
   343	    }
   344	
   345	    #[test]
   346	    fn cache_file_name_is_plan_prefixed_json() {
   347	        let name = cache_file_name(Path::new("/home/u/repo"));
   348	        assert!(name.starts_with("plan-"), "name: {name}");
   349	        assert!(name.ends_with(".json"), "name: {name}");
   350	        assert!(!name.contains('/'), "single path component, not /tmp/...");
   351	    }
   352	
   353	    #[test]
   354	    fn fingerprint_is_stable_for_same_inputs() {
   355	        let e = entries(&[("a.rs", "h1"), ("b.rs", "h2")]);
   356	        assert_eq!(
   357	            digest_fingerprint("groq:m", &e),
   358	            digest_fingerprint("groq:m", &e)
   359	        );
   360	    }
   361	
   362	    #[test]
   363	    fn fingerprint_flips_on_content_change() {
   364	        let before = entries(&[("a.rs", "h1")]);
   365	        let after = entries(&[("a.rs", "h2")]); // same name, different content hash
   366	        assert_ne!(
   367	            digest_fingerprint("m", &before),
   368	            digest_fingerprint("m", &after),
   369	            "a content change (not a name change) must invalidate"
   370	        );
   371	    }
   372	
   373	    #[test]
   374	    fn fingerprint_flips_on_file_set_change() {
   375	        let one = entries(&[("a.rs", "h1")]);
   376	        let two = entries(&[("a.rs", "h1"), ("b.rs", "h2")]);
   377	        assert_ne!(digest_fingerprint("m", &one), digest_fingerprint("m", &two));
   378	    }
   379	
   380	    #[test]
   381	    fn fingerprint_flips_on_model_change() {
   382	        let e = entries(&[("a.rs", "h1")]);
   383	        assert_ne!(
   384	            digest_fingerprint("groq:model-a", &e),
   385	            digest_fingerprint("groq:model-b", &e),
   386	            "switching provider/model must invalidate"
   387	        );
   388	    }
   389	
   390	    #[test]
   391	    fn deletion_marker_differs_from_a_real_hash() {
   392	        // A pending deletion must not collide with any content hash.
   393	        let present = entries(&[("a.rs", "deadbeef")]);
   394	        let deleted = entries(&[("a.rs", "\0DELETED")]);
   395	        assert_ne!(
   396	            digest_fingerprint("m", &present),
   397	            digest_fingerprint("m", &deleted)
   398	        );
   399	    }
   400	
   401	    #[test]
   402	    fn hash_path_hashes_regular_file_content() {
   403	        let dir = tempfile::tempdir().unwrap();
   404	        let p = dir.path().join("f.txt");
   405	        std::fs::write(&p, b"hello").unwrap();
   406	        let h = hash_path(&p);
   407	        assert_eq!(h.len(), 64, "regular file -> hex sha256");
   408	        let p2 = dir.path().join("g.txt");
   409	        std::fs::write(&p2, b"hello").unwrap();
   410	        assert_eq!(h, hash_path(&p2), "same content -> same hash");

 succeeded in 0ms:
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
   210	    // plan (FR-28) - but only on the REAL (non-dry-run) path. A `--dry-run`
   211	    // (incl. `--all --dry-run` and a dry-run fallback) returns above and clears
   212	    // nothing: a preview must mutate no state (FR-7). A stale cache left behind
   213	    // by a dry-run is harmless - the next real run re-validates the fingerprint
   214	    // and re-analyzes on a mismatch.
   215	    cache::clear(repo);
   216	    let snapshot = repo.snapshot_index()?;
   217	    let result = single_commit_flow(repo, args);
   218	    if result.is_err() {
   219	        let _ = repo.restore_index(&snapshot);
   220	    }
   221	    result
   222	}
   223	
   224	fn single_commit_flow(repo: &Repo, args: &Cli) -> Result<(), GcmError> {
   225	    let gathered = diff::gather(repo)?;

exec
/bin/zsh -lc "nl -ba scripts/acceptance.sh | sed -n '520,710p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
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
   531	
   532	note "AC-C3: rejecting pre-commit hook leaves the group staged + plan un-advanced (AC-3, FR-58)"
   533	reset_cache; d="$(cache_repo_2group)"
   534	mkdir -p "$d/.git/hooks"
   535	printf '#!/bin/sh\nexit 1\n' > "$d/.git/hooks/pre-commit"; chmod +x "$d/.git/hooks/pre-commit"
   536	( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   537	[ $rc -ne 0 ] && ok "rejecting hook -> exit $rc" || bad "expected non-zero on hook rejection"
   538	grep -qi "left staged" /tmp/gcm-out && ok "error explains the group is left staged" || bad "FR-58 message missing"
   539	git -C "$d" diff --cached --name-only | grep -qx 'src.txt' && ok "group 1 left staged for retry" || bad "group 1 not staged after hook reject"
   540	cf="$(cache_file)"; before="$(cat "$cf" 2>/dev/null)"
   541	{ [ -n "$before" ] && printf '%s' "$before" | grep -q '"src.txt"' && printf '%s' "$before" | grep -q '"docs.md"'; } && ok "cache un-advanced (still the full plan: both groups)" || bad "cache not the full un-advanced plan"
   542	[ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "no commit created" || bad "a commit slipped through the rejecting hook"
   543	# A second rejected run must not mutate the cache (idempotent; never advances).
   544	( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   545	[ "$before" = "$(cat "$cf" 2>/dev/null)" ] && ok "cache byte-identical after a repeated rejected commit" || bad "cache changed across rejected retries"
   546	# Removing the hook and re-running retries the same group from the cache.
   547	rm -f "$d/.git/hooks/pre-commit"; : > "$CAPTURE"; : > "$PLAN_FILE"
   548	( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   549	if [ "$SIGNING_OK" -eq 1 ]; then
   550	  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'src.txt' && ok "retry committed the same group 1 from cache" || bad "retry did not commit group 1"
   551	else
   552	  skip "AC-C3 retry-commit assertion needs signing"
   553	fi
   554	reset_cache; rm -rf "$d"
   555	
   556	note "AC-C4: first commit in an unborn repo (no HEAD) works with the cache (AC-4)"
   557	if [ "$SIGNING_OK" -eq 1 ]; then
   558	  reset_cache; d="$(new_repo)"   # fresh repo, no commits -> unborn HEAD
   559	  printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
   560	  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"},{"files":["b.txt"],"summary":"b","commit_message":null}]}' > "$PLAN_FILE"
   561	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   562	  [ $rc -eq 0 ] && ok "unborn first commit exit 0" || bad "unborn run (rc=$rc; $(tail -1 /tmp/gcm-out))"
   563	  git -C "$d" rev-parse HEAD >/dev/null 2>&1 && ok "HEAD now exists (first commit created)" || bad "no HEAD after run"
   564	  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'a.txt' && ok "group 1 (a.txt) committed" || bad "a.txt not committed"
   565	  [ -n "$(cache_file)" ] && ok "cache advanced to group 2" || bad "no cache after unborn first commit"
   566	  reset_cache; rm -rf "$d"
   567	else
   568	  skip "AC-C4 needs signing"
   569	fi
   570	
   571	note "AC-C5: cache file lives in the cache dir, named plan-<key>.json, mode 0600 (AC-5, FR-29)"
   572	if [ "$SIGNING_OK" -eq 1 ]; then
   573	  reset_cache; d="$(cache_repo_2group)"
   574	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   575	  cf="$(cache_file)"
   576	  [ -n "$cf" ] && [ -f "$cf" ] && ok "cache file created under the configured cache dir" || bad "no cache file produced"
   577	  case "$cf" in "$GCM_CACHE_DIR"/plan-*.json) ok "name is plan-<key>.json under GCM_CACHE_DIR" ;; *) bad "unexpected cache path: $cf" ;; esac
   578	  mode="$(stat -f '%Lp' "$cf" 2>/dev/null || stat -c '%a' "$cf" 2>/dev/null)"
   579	  [ "$mode" = "600" ] && ok "cache file mode is 0600" || bad "cache file mode is '$mode' (want 600)"
   580	  reset_cache; rm -rf "$d"
   581	else
   582	  skip "AC-C5 needs signing"
   583	fi
   584	
   585	note "AC-C6: --reset re-analyzes; --all clears the cache (AC-6, FR-8/FR-28)"
   586	if [ "$SIGNING_OK" -eq 1 ]; then
   587	  reset_cache; d="$(cache_repo_2group)"
   588	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   589	  [ -n "$(cache_file)" ] && ok "cache warmed (group 2 cached)" || bad "no cache after run 1"
   590	  : > "$CAPTURE"
   591	  printf '%s' '{"groups":[{"files":["docs.md"],"summary":"docs","commit_message":"docs: d"}]}' > "$PLAN_FILE"
   592	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --reset --yes >/tmp/gcm-out 2>&1 )
   593	  grep -q '"response_format"' "$CAPTURE" && ok "--reset forced a grouping call" || bad "--reset did not re-analyze"
   594	  reset_cache; rm -rf "$d"
   595	
   596	  reset_cache; d="$(cache_repo_2group)"
   597	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   598	  [ -n "$(cache_file)" ] && ok "cache warmed before --all" || bad "no cache to clear"
   599	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --yes >/tmp/gcm-out 2>&1 )
   600	  [ -z "$(cache_file)" ] && ok "--all cleared the cache" || bad "--all left the cache in place"
   601	  reset_cache; rm -rf "$d"
   602	else
   603	  skip "AC-C6 needs signing"
   604	fi
   605	
   606	note "AC-C7: aborting at the prompt leaves the cache un-advanced (AC-7)"
   607	if [ "$SIGNING_OK" -eq 1 ] && command -v expect >/dev/null 2>&1; then
   608	  reset_cache; d="$(cache_repo_2group)"
   609	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   610	  before="$(cat "$(cache_file)")"
   611	  GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" GCM_BIN="$BIN" GCM_DIR="$d" expect -c '
   612	    set timeout 20
   613	    spawn -noecho sh -c "cd $env(GCM_DIR) && GROQ_API_KEY=$env(GROQ_API_KEY) GCM_GROQ_BASE_URL=$env(GCM_GROQ_BASE_URL) $env(GCM_BIN)"
   614	    expect {
   615	      -re {\[Y/n/e} { send "n\r" }
   616	      timeout { exit 3 }
   617	    }
   618	    expect eof
   619	  ' >/tmp/gcm-out 2>&1
   620	  after="$(cat "$(cache_file)")"
   621	  [ "$before" = "$after" ] && ok "cache byte-identical after abort (not advanced)" || bad "abort changed/advanced the cache"
   622	  git -C "$d" status --porcelain | grep -q 'docs.md' && ok "group 2 still pending after abort" || bad "group 2 not pending after abort"
   623	  reset_cache; rm -rf "$d"
   624	else
   625	  skip "AC-C7 needs signing + expect"
   626	fi
   627	
   628	note "AC-C11: a single-group plan deletes the cache after its commit (eval 11)"
   629	if [ "$SIGNING_OK" -eq 1 ]; then
   630	  reset_cache; d="$(new_repo)"
   631	  printf 'v1\n' > "$d/only.txt"
   632	  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
   633	  git -C "$d" -c commit.gpgsign=false commit -qm init
   634	  printf 'v2\n' > "$d/only.txt"
   635	  printf '%s' '{"groups":[{"files":["only.txt"],"summary":"only","commit_message":"feat: only"}]}' > "$PLAN_FILE"
   636	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   637	  [ -z "$(cache_file)" ] && ok "single-group plan left no cache (nothing to advance to)" || bad "cache lingered after the last group"
   638	  reset_cache; rm -rf "$d"
   639	else
   640	  skip "AC-C11 needs signing"
   641	fi
   642	
   643	note "AC-C21: a group's message excludes other groups' untracked files (blind-spot #3, eval 21)"
   644	if [ "$SIGNING_OK" -eq 1 ]; then
   645	  reset_cache; d="$(new_repo)"
   646	  printf 'seed\n' > "$d/seed.txt"
   647	  git -C "$d" -c commit.gpgsign=false add -A >/dev/null
   648	  git -C "$d" -c commit.gpgsign=false commit -qm init
   649	  # Three untracked files in three groups. After group 1 commits, groups 2 AND 3
   650	  # are still untracked, so the message-only call for group 2 must exclude g3.
   651	  printf 'G1_CONTENT\n' > "$d/g1.txt"
   652	  printf 'G2_CONTENT\n' > "$d/g2.txt"
   653	  printf 'G3_CONTENT\n' > "$d/g3.txt"
   654	  printf '%s' '{"groups":[{"files":["g1.txt"],"summary":"g1","commit_message":"feat: g1"},{"files":["g2.txt"],"summary":"g2","commit_message":null},{"files":["g3.txt"],"summary":"g3","commit_message":null}]}' > "$PLAN_FILE"
   655	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   656	  # Run 2: cache hit, group 0 = g2 (null msg) -> message-only call scoped to g2,
   657	  # while g3 is still untracked. The request body must contain g2 but not g3.
   658	  : > "$CAPTURE"; : > "$PLAN_FILE"
   659	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   660	  grep -q 'G2_CONTENT' "$CAPTURE" && ok "scoped message includes the group's own untracked file" || bad "group 2 content missing from its message diff"
   661	  grep -q 'G3_CONTENT' "$CAPTURE" && bad "another group's untracked content leaked into the message diff" || ok "other groups' untracked content excluded (filter works)"
   662	  reset_cache; rm -rf "$d"
   663	else
   664	  skip "AC-C21 needs signing"
   665	fi
   666	
   667	note "AC-C-rename: renaming a pending file invalidates the cache (eval 4)"
   668	if [ "$SIGNING_OK" -eq 1 ]; then
   669	  reset_cache; d="$(cache_repo_2group)"
   670	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   671	  ( cd "$d" && git mv docs.md docs2.md )   # rename the still-pending group-2 file
   672	  printf '%s' '{"groups":[{"files":["docs2.md"],"summary":"docs","commit_message":"docs: renamed"}]}' > "$PLAN_FILE"
   673	  : > "$CAPTURE"
   674	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   675	  grep -q '"response_format"' "$CAPTURE" && ok "rename invalidated the cache -> grouping call" || bad "stale cache reused after a rename"
   676	  reset_cache; rm -rf "$d"
   677	else
   678	  skip "AC-C-rename needs signing"
   679	fi
   680	
   681	note "AC-C-hookfix: a hook that reformats+restages lets the commit succeed and the cache advance (eval 6)"
   682	if [ "$SIGNING_OK" -eq 1 ]; then
   683	  reset_cache; d="$(cache_repo_2group)"
   684	  mkdir -p "$d/.git/hooks"
   685	  printf '#!/bin/sh\nprintf "reformatted\\n" > src.txt\ngit add src.txt\nexit 0\n' > "$d/.git/hooks/pre-commit"
   686	  chmod +x "$d/.git/hooks/pre-commit"
   687	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   688	  [ $rc -eq 0 ] && ok "hook reformat+restage -> commit succeeds" || bad "reformatting hook run (rc=$rc; $(tail -1 /tmp/gcm-out))"
   689	  git -C "$d" show HEAD:src.txt | grep -q 'reformatted' && ok "committed the hook's reformatted content" || bad "reformatted content not committed"
   690	  cf="$(cache_file)"
   691	  [ -n "$cf" ] && grep -q '"docs.md"' "$cf" && ok "cache advanced to group 2 after the hook-fixed commit" || bad "cache did not advance after a successful commit"
   692	  reset_cache; rm -rf "$d"
   693	else
   694	  skip "AC-C-hookfix needs signing"
   695	fi
   696	
   697	note "AC-C-untracked: an untracked-only cached group commits on the next run (eval 18)"
   698	if [ "$SIGNING_OK" -eq 1 ]; then
   699	  reset_cache; d="$(new_repo)"
   700	  printf 'seed\n' > "$d/seed.txt"; git -C "$d" -c commit.gpgsign=false add -A >/dev/null
   701	  git -C "$d" -c commit.gpgsign=false commit -qm init
   702	  printf 'A\n' > "$d/g1.txt"; printf 'B\n' > "$d/g2.txt"   # both untracked
   703	  printf '%s' '{"groups":[{"files":["g1.txt"],"summary":"g1","commit_message":"feat: g1"},{"files":["g2.txt"],"summary":"g2","commit_message":null}]}' > "$PLAN_FILE"
   704	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   705	  : > "$CAPTURE"; : > "$PLAN_FILE"
   706	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   707	  [ $rc -eq 0 ] && ok "untracked cached group commit exit 0" || bad "untracked cached group (rc=$rc; $(tail -1 /tmp/gcm-out))"
   708	  grep -q '"response_format"' "$CAPTURE" && bad "made a grouping call (cache missed)" || ok "no grouping call (cache hit on the untracked group)"
   709	  git -C "$d" show --name-only --pretty=format: HEAD | grep -qx 'g2.txt' && ok "untracked group 2 committed from cache" || bad "g2.txt not committed"
   710	  reset_cache; rm -rf "$d"

exec
/bin/zsh -lc "nl -ba scripts/acceptance.sh | sed -n '710,780p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   710	  reset_cache; rm -rf "$d"
   711	else
   712	  skip "AC-C-untracked needs signing"
   713	fi
   714	
   715	note "AC-C-delete: a deletion-only cached group commits the removal (eval 17)"
   716	if [ "$SIGNING_OK" -eq 1 ]; then
   717	  reset_cache; d="$(new_repo)"
   718	  printf 'a\n' > "$d/a.txt"; printf 'b\n' > "$d/b.txt"
   719	  git -C "$d" -c commit.gpgsign=false add -A >/dev/null; git -C "$d" -c commit.gpgsign=false commit -qm init
   720	  printf 'a2\n' > "$d/a.txt"; rm "$d/b.txt"   # group 1 modifies a.txt, group 2 deletes b.txt
   721	  printf '%s' '{"groups":[{"files":["a.txt"],"summary":"a","commit_message":"feat: a"},{"files":["b.txt"],"summary":"rm b","commit_message":null}]}' > "$PLAN_FILE"
   722	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   723	  : > "$CAPTURE"; : > "$PLAN_FILE"
   724	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   725	  [ $rc -eq 0 ] && ok "deletion cached group commit exit 0" || bad "deletion cached group (rc=$rc; $(tail -1 /tmp/gcm-out))"
   726	  git -C "$d" ls-files | grep -qx 'b.txt' && bad "b.txt still tracked (deletion not committed)" || ok "b.txt deletion committed from cache"
   727	  reset_cache; rm -rf "$d"
   728	else
   729	  skip "AC-C-delete needs signing"
   730	fi
   731	
   732	note "AC-C-fallback: a grouping fallback clears the cache (eval 10 fallback half)"
   733	if [ "$SIGNING_OK" -eq 1 ]; then
   734	  reset_cache; d="$(cache_repo_2group)"
   735	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   736	  [ -n "$(cache_file)" ] && ok "cache warmed before fallback" || bad "no cache to clear"
   737	  printf 'edited\n' > "$d/docs.md"               # invalidate -> next run is a miss
   738	  printf '%s' '{ not valid json' > "$PLAN_FILE"   # grouping returns malformed -> fallback
   739	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   740	  grep -qi "Falling back" /tmp/gcm-out && ok "malformed plan -> fallback" || bad "no fallback on malformed plan"
   741	  [ -z "$(cache_file)" ] && ok "fallback cleared the cache" || bad "fallback left the cache in place"
   742	  reset_cache; rm -rf "$d"
   743	else
   744	  skip "AC-C-fallback needs signing"
   745	fi
   746	
   747	note "AC-C-drynoclear: --all --dry-run previews without clearing the cache (FR-7 no-mutation)"
   748	if [ "$SIGNING_OK" -eq 1 ]; then
   749	  reset_cache; d="$(cache_repo_2group)"
   750	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --yes >/tmp/gcm-out 2>&1 )
   751	  before="$(cat "$(cache_file)" 2>/dev/null)"
   752	  ( cd "$d" && GROQ_API_KEY=dummy GCM_GROQ_BASE_URL="$MOCK_URL" "$BIN" --all --dry-run >/tmp/gcm-out 2>&1 )
   753	  after="$(cat "$(cache_file)" 2>/dev/null)"
   754	  { [ -n "$before" ] && [ "$before" = "$after" ]; } && ok "--all --dry-run left the cache untouched" || bad "--all --dry-run mutated the cache"
   755	  reset_cache; rm -rf "$d"
   756	else
   757	  skip "AC-C-drynoclear needs signing"
   758	fi
   759	
   760	stop_mock
   761	
   762	# --- optional real-network smoke test --------------------------------------
   763	if [ "${GCM_LIVE:-0}" = "1" ] && [ -n "${GROQ_API_KEY:-}" ]; then
   764	  note "LIVE: real Groq call (GCM_LIVE=1)"
   765	  if [ "$SIGNING_OK" -eq 1 ]; then
   766	    d="$(new_repo)"; echo "live test change" > "$d/live.txt"
   767	    ( cd "$d" && "$BIN" --yes >/tmp/gcm-out 2>&1 ); rc=$?
   768	    [ $rc -eq 0 ] && [ "$(git -C "$d" log --oneline | wc -l | tr -d ' ')" = "1" ] && ok "live Groq -> one signed commit" || bad "live run (rc=$rc; $(tail -2 /tmp/gcm-out))"
   769	    rm -rf "$d"
   770	  else
   771	    skip "live test needs working signing"
   772	  fi
   773	fi
   774	
   775	printf '\n\033[1m== Summary ==\033[0m  PASS=%d FAIL=%d SKIP=%d\n' "$PASS" "$FAIL" "$SKIP"
   776	[ "$FAIL" -eq 0 ]

codex
I’ve isolated one concrete test gap in the new acceptance coverage and I’m double-checking whether there are any behavior bugs beyond that. I’m pulling exact line references now so the verdict can be precise rather than generic.
exec
/bin/zsh -lc "nl -ba src/cache.rs | sed -n '400,455p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   400	
   401	    #[test]
   402	    fn hash_path_hashes_regular_file_content() {
   403	        let dir = tempfile::tempdir().unwrap();
   404	        let p = dir.path().join("f.txt");
   405	        std::fs::write(&p, b"hello").unwrap();
   406	        let h = hash_path(&p);
   407	        assert_eq!(h.len(), 64, "regular file -> hex sha256");
   408	        let p2 = dir.path().join("g.txt");
   409	        std::fs::write(&p2, b"hello").unwrap();
   410	        assert_eq!(h, hash_path(&p2), "same content -> same hash");
   411	        std::fs::write(&p2, b"world").unwrap();
   412	        assert_ne!(h, hash_path(&p2), "different content -> different hash");
   413	    }
   414	
   415	    #[test]
   416	    fn hash_path_missing_file_is_deleted_marker() {
   417	        let dir = tempfile::tempdir().unwrap();
   418	        assert_eq!(hash_path(&dir.path().join("nope")), "\0DELETED");
   419	    }
   420	
   421	    #[cfg(unix)]
   422	    #[test]
   423	    fn hash_path_does_not_follow_symlinks() {
   424	        use std::os::unix::fs::symlink;
   425	        let dir = tempfile::tempdir().unwrap();
   426	        let target = dir.path().join("target.txt");
   427	        std::fs::write(&target, b"secret-bytes").unwrap();
   428	        let link = dir.path().join("link.txt");
   429	        symlink(&target, &link).unwrap();
   430	        // A symlink is hashed by its target PATH, not the pointed-to bytes - so
   431	        // it must differ from the regular-file hash of that content (proving the
   432	        // link was not followed: no FIFO-block, no out-of-repo content leak).
   433	        assert_ne!(
   434	            hash_path(&link),
   435	            hash_path(&target),
   436	            "symlink must not be followed into its target's content"
   437	        );
   438	        let link2 = dir.path().join("link2.txt");
   439	        symlink(&target, &link2).unwrap();
   440	        assert_eq!(
   441	            hash_path(&link),
   442	            hash_path(&link2),
   443	            "same target path -> same symlink hash"
   444	        );
   445	    }
   446	
   447	    #[test]
   448	    fn remaining_groups_drops_the_first() {
   449	        let plan = Plan {
   450	            groups: vec![
   451	                group(&["a.rs"], Some("feat: a")),
   452	                group(&["b.rs"], None),
   453	                group(&["c.rs"], None),
   454	            ],
   455	        };

exec
/bin/zsh -lc 'rg -n "append_untracked|not a regular file|symlink_metadata|read_link|SPECIAL|UNREADABLE|DELETED" src' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
src/diff.rs:66:    append_untracked(repo, &mut body, None)?;
src/diff.rs:83:    append_untracked(repo, &mut body, Some(&allow))?;
src/diff.rs:104:    append_untracked(repo, &mut body, None)?;
src/diff.rs:121:fn append_untracked(
src/diff.rs:149:        // Only read regular files. `symlink_metadata` does not follow symlinks,
src/diff.rs:152:        let is_regular = std::fs::symlink_metadata(&full)
src/diff.rs:157:                "\n--- /dev/null\n+++ b/{path}\n[omitted: not a regular file]\n"
src/cache.rs:200:/// `diff::append_untracked`): `symlink_metadata` does not traverse the link, so
src/cache.rs:214:    let meta = match fs::symlink_metadata(full) {
src/cache.rs:216:        Err(_) => return "\0DELETED".to_string(),
src/cache.rs:220:        return match fs::read_link(full) {
src/cache.rs:227:            Err(_) => "\0UNREADABLE".to_string(),
src/cache.rs:232:        return "\0SPECIAL".to_string();
src/cache.rs:236:        Err(_) => return "\0DELETED".to_string(),
src/cache.rs:245:            Err(_) => return "\0UNREADABLE".to_string(),
src/cache.rs:394:        let deleted = entries(&[("a.rs", "\0DELETED")]);
src/cache.rs:418:        assert_eq!(hash_path(&dir.path().join("nope")), "\0DELETED");

exec
/bin/zsh -lc 'rg -n "fn changed_files|struct ChangedFile|is_unmerged|stage_group|clear_staged|snapshot_index|restore_index|commit_signed|stage_all" src' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
src/main.rs:62:    if changed.iter().any(|c| c.is_unmerged()) {
src/main.rs:163:    let snapshot = repo.snapshot_index()?;
src/main.rs:167:            let _ = repo.restore_index(&snapshot);
src/main.rs:191:            repo.clear_staged()?;
src/main.rs:192:            repo.stage_group(group1_files)?;
src/main.rs:193:            repo.commit_signed(&final_message)?;
src/main.rs:216:    let snapshot = repo.snapshot_index()?;
src/main.rs:219:        let _ = repo.restore_index(&snapshot);
src/main.rs:233:            repo.stage_all()?;
src/main.rs:234:            repo.commit_signed(&final_message)?;
src/git.rs:197:    pub fn snapshot_index(&self) -> Result<String, GcmError> {
src/git.rs:203:    pub fn restore_index(&self, tree: &str) -> Result<(), GcmError> {
src/git.rs:208:    pub fn stage_all(&self) -> Result<(), GcmError> {
src/git.rs:220:    pub fn commit_signed(&self, message: &str) -> Result<(), GcmError> {
src/git.rs:240:    pub fn changed_files(&self) -> Result<Vec<ChangedFile>, GcmError> {
src/git.rs:255:    /// [`ChangedFile::is_unmerged`] this distinguishes a clean merge (commit it)
src/git.rs:270:    pub fn clear_staged(&self) -> Result<(), GcmError> {
src/git.rs:285:    pub fn stage_group(&self, files: &[&ChangedFile]) -> Result<(), GcmError> {
src/git.rs:324:pub struct ChangedFile {
src/git.rs:334:    pub fn is_unmerged(&self) -> bool {
src/git.rs:424:        assert!(parse_status_z(b"UU conflict.txt\0")[0].is_unmerged());
src/git.rs:425:        assert!(parse_status_z(b"AA both-added.txt\0")[0].is_unmerged());
src/git.rs:426:        assert!(!parse_status_z(b" M ok.txt\0")[0].is_unmerged());
src/git.rs:427:        assert!(!parse_status_z(b"?? new.txt\0")[0].is_unmerged());
src/git.rs:478:    fn stage_group_isolates_literal_glob_filename() {
src/git.rs:491:        repo.clear_staged().unwrap();
src/git.rs:492:        repo.stage_group(&[cf_for(&files, "a*.txt")]).unwrap();
src/git.rs:498:    fn stage_group_completes_a_rename() {
src/git.rs:511:        repo.clear_staged().unwrap();
src/git.rs:512:        repo.stage_group(&[rename]).unwrap();
src/git.rs:522:    fn stage_group_stages_a_deletion() {
src/git.rs:531:        repo.clear_staged().unwrap();
src/git.rs:532:        repo.stage_group(&[cf_for(&files, "del.txt")]).unwrap();
src/git.rs:542:    fn clear_staged_resets_index_to_head() {
src/git.rs:555:        repo.clear_staged().unwrap();
src/git.rs:560:    fn changed_files_flags_a_merge_conflict() {
src/git.rs:579:            files.iter().any(|c| c.is_unmerged()),

exec
/bin/zsh -lc "nl -ba src/diff.rs | sed -n '121,170p'" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
   121	fn append_untracked(
   122	    repo: &Repo,
   123	    body: &mut String,
   124	    allow: Option<&HashSet<String>>,
   125	) -> Result<(), GcmError> {
   126	    let mut untracked = repo.untracked_files()?;
   127	    untracked.sort();
   128	
   129	    // Every (allow-listed) untracked path counts toward the file-count cap -
   130	    // binary and unreadable files included - so a directory of thousands of
   131	    // files (of any kind) cannot force thousands of reads. Once either cap is
   132	    // reached, every remaining file is listed by name only, with no read at all
   133	    // (FR-57).
   134	    let mut files_done = 0usize;
   135	    let mut bytes_used = 0usize;
   136	    for path in &untracked {
   137	        // Filter to the allow-list (if any) before the caps, so excluded paths
   138	        // neither consume the budget nor reach the prompt.
   139	        if allow.is_some_and(|a| !a.contains(path)) {
   140	            continue;
   141	        }
   142	        if files_done >= MAX_UNTRACKED_FILES || bytes_used >= MAX_UNTRACKED_BYTES {
   143	            body.push_str(&format!(
   144	                "\n--- /dev/null\n+++ b/{path}\n[content omitted: untracked cap reached]\n"
   145	            ));
   146	            continue;
   147	        }
   148	        let full = repo.root().join(path);
   149	        // Only read regular files. `symlink_metadata` does not follow symlinks,
   150	        // so we never read a symlink's target (which could leak content from
   151	        // outside the repo) and never block on a FIFO/device/socket.
   152	        let is_regular = std::fs::symlink_metadata(&full)
   153	            .map(|m| m.file_type().is_file())
   154	            .unwrap_or(false);
   155	        if !is_regular {
   156	            body.push_str(&format!(
   157	                "\n--- /dev/null\n+++ b/{path}\n[omitted: not a regular file]\n"
   158	            ));
   159	            files_done += 1;
   160	            continue;
   161	        }
   162	        // Read at most a per-file slice bounded by the remaining byte budget, so
   163	        // a single huge file is never loaded into memory in full.
   164	        let budget = (MAX_UNTRACKED_BYTES - bytes_used).min(PER_FILE_BYTES);
   165	        match read_capped(&full, budget) {
   166	            Ok((content, more)) if looks_binary(&content) => {
   167	                body.push_str(&format!("\n--- /dev/null\n+++ b/{path}\n+[binary file]\n"));
   168	                let _ = more;
   169	            }
   170	            Ok((content, more)) => {

exec
/bin/zsh -lc 'rg -n "AC-safe-files|not a regular file|did not hang on FIFO" scripts/acceptance.sh' in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
163:note "AC-safe-files: untracked symlink/FIFO are name-only (no follow, no freeze)"
170:[ "$rc" -ne 124 ] && ok "did not hang on FIFO (rc=$rc)" || bad "hung on FIFO (timeout)"
172:grep -q "not a regular file" "$CAPTURE" && ok "special files listed name-only" || bad "no name-only marker for special files"

exec
/bin/zsh -lc "sed -n '232,320p' src/git.rs" in /Users/mk/Code/gcm--feat-clo-491-plan-cache
 succeeded in 0ms:
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
            return Err(GcmError::Git(format!(
                "git add failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(())
    }
}

/// One entry from `git status --porcelain=v1 -z`: the two status chars (`x`

codex
## Verdict: PASS_WITH_NOTES

## Resolved
- `HIGH` is resolved in code. [src/cache.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/cache.rs:206) now routes `content_hash()` through `hash_path()`, uses `symlink_metadata()` so links are not followed, hashes symlinks from `read_link()` target text, returns `\0SPECIAL` for non-regular files before any `open()`, and streams regular files via `BufReader` with a fixed 64 KiB buffer.
- `MEDIUM` is resolved, and I accept the documented behavior change. [src/main.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/main.rs:202) returns before `cache::clear()` on any dry-run, so `--all --dry-run` mutates nothing per FR-7. Real `--all` still clears in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:585), real fallback still clears in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:732), and `AC-C-drynoclear` locks the no-clear behavior in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:747).

## New Issues
- `LOW` [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:681) `AC-C-hookfix` does not actually prove cache advancement. It only checks that the cache mentions `docs.md`; a stale full-plan cache containing both `src.txt` and `docs.md` would still pass.
- `LOW` [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:715) `AC-C-delete` does not prove the deletion-only group was replayed from cache. After blanking `$PLAN_FILE`, a cache miss would fall back to single-commit and the test would still pass if `b.txt` simply ended up deleted.

## Notes
- The three new `hash_path_*` unit tests in [src/cache.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/cache.rs:401) prove regular-file hashing, missing-file handling, and symlink no-follow. They do not directly cover the special-file marker branch; that branch is implemented in [src/cache.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/cache.rs:213) and only indirectly echoed by the older `AC-safe-files` check in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:163).
- The added cases that are correct and meaningful as written are rename invalidation [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:667), untracked-only replay [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:697), fallback-clears [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:732), `AC-C3` byte-identical [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:543), and `AC-C-drynoclear` [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:747).
- Static review only; I did not run `cargo test` or `scripts/acceptance.sh` in this read-only workspace.
hook: Stop
hook: Stop Completed
tokens used
56,536
## Verdict: PASS_WITH_NOTES

## Resolved
- `HIGH` is resolved in code. [src/cache.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/cache.rs:206) now routes `content_hash()` through `hash_path()`, uses `symlink_metadata()` so links are not followed, hashes symlinks from `read_link()` target text, returns `\0SPECIAL` for non-regular files before any `open()`, and streams regular files via `BufReader` with a fixed 64 KiB buffer.
- `MEDIUM` is resolved, and I accept the documented behavior change. [src/main.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/main.rs:202) returns before `cache::clear()` on any dry-run, so `--all --dry-run` mutates nothing per FR-7. Real `--all` still clears in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:585), real fallback still clears in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:732), and `AC-C-drynoclear` locks the no-clear behavior in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:747).

## New Issues
- `LOW` [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:681) `AC-C-hookfix` does not actually prove cache advancement. It only checks that the cache mentions `docs.md`; a stale full-plan cache containing both `src.txt` and `docs.md` would still pass.
- `LOW` [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:715) `AC-C-delete` does not prove the deletion-only group was replayed from cache. After blanking `$PLAN_FILE`, a cache miss would fall back to single-commit and the test would still pass if `b.txt` simply ended up deleted.

## Notes
- The three new `hash_path_*` unit tests in [src/cache.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/cache.rs:401) prove regular-file hashing, missing-file handling, and symlink no-follow. They do not directly cover the special-file marker branch; that branch is implemented in [src/cache.rs](/Users/mk/Code/gcm--feat-clo-491-plan-cache/src/cache.rs:213) and only indirectly echoed by the older `AC-safe-files` check in [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:163).
- The added cases that are correct and meaningful as written are rename invalidation [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:667), untracked-only replay [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:697), fallback-clears [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:732), `AC-C3` byte-identical [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:543), and `AC-C-drynoclear` [scripts/acceptance.sh](/Users/mk/Code/gcm--feat-clo-491-plan-cache/scripts/acceptance.sh:747).
- Static review only; I did not run `cargo test` or `scripts/acceptance.sh` in this read-only workspace.
