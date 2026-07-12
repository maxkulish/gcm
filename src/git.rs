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

/// Outcome of [`Repo::finish_conflict_op`], classified by postconditions
/// (operation refs and unmerged entries re-read after the subprocess exits),
/// never by parsing git's output text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FinishOutcome {
    /// The operation finished: no conflict state remains, no unmerged entries.
    /// Carries the new short HEAD sha for the report.
    Completed { head_sha: String },
    /// A rebase/cherry-pick continued past this stop and halted on the next
    /// conflicted commit in its sequence (CLO-554 handles looping).
    StoppedOnNextConflict,
    /// No merge/rebase/cherry-pick is in progress (e.g. `git checkout -m`
    /// conflicts) - there is nothing to finish.
    NothingToFinish,
    /// The finishing command failed (rejecting hook, signing failure, ...);
    /// the operation is still in progress and staged state is untouched.
    Failed { op: &'static str },
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

    /// Construct a `Repo` at an explicit path (used by tests outside this module).
    #[allow(dead_code)]
    pub(crate) fn at_root(root: PathBuf) -> Repo {
        Repo { root }
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

    /// The full SHA of HEAD after a successful commit.
    pub fn last_commit_hash(&self) -> Result<String, GcmError> {
        self.capture(&["rev-parse", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    /// Return the URL of a remote (e.g. `origin`), if set.
    pub fn remote_url(&self, name: &str) -> Result<Option<String>, GcmError> {
        let out = self
            .git(&["remote", "get-url", name])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git remote get-url: {e}")))?;
        if !out.status.success() {
            return Ok(None);
        }
        Ok(Some(
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        ))
    }

    /// Run a git command in the repo and return Ok if it exits 0. Non-zero
    /// exit becomes a [`GcmError::Git`] with the captured stderr.
    pub fn run_git(&self, args: &[&str]) -> Result<(), GcmError> {
        let out = self
            .git(args)
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git {}: {e}", args.join(" "))))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("gcm: git {}: {}", args.join(" "), stderr);
            return Err(GcmError::Git(format!(
                "git {} failed: {}",
                args.join(" "),
                stderr.trim()
            )));
        }
        Ok(())
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

    /// Diff `--stat` scoped to specific paths (CLO-491 per-group message header).
    /// With HEAD, `git diff HEAD -- <paths>` covers tracked changes. On an unborn
    /// branch, combine unstaged and staged scoped diffs so staged-then-modified
    /// files are represented. Empty `paths` returns an empty string rather than
    /// an unscoped whole-tree diff.
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
    /// body). Same HEAD/unborn handling as [`Self::diff_stat_for`]. Empty
    /// `paths` returns an empty string.
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

    /// Create a signed commit (FR-4). Stdin is inherited so GPG/SSH passphrase
    /// (pinentry) prompts work on the user's terminal. Stdout is piped and kept
    /// off the main stdout stream: in `--json` mode the consumer expects a
    /// single JSON object, and in plain mode we print our own outcome text.
    pub fn commit_signed(&self, message: &str) -> Result<(), GcmError> {
        let output = self
            .git(&["commit", "-S", "-m", message])
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git commit: {e}")))?;
        if !output.status.success() {
            return Err(GcmError::CommitFailed(
                "git commit failed (see output above)".to_string(),
            ));
        }
        // Any git commit summary output is a log-line, not machine output.
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            eprintln!("{stdout}");
        }
        Ok(())
    }

    /// Finish the in-progress conflict operation once every resolution is
    /// staged. A merge is committed with `git commit -S --no-edit` (consumes
    /// the prepared MERGE_MSG; FR-4 signing preserved); a rebase/cherry-pick
    /// continues with `-c commit.gpgsign=true <op> --continue` so its commits
    /// are signed too. `GIT_EDITOR=true` suppresses message editors; stdin and
    /// stderr are inherited (the `commit_signed` pattern) so pinentry and hook
    /// output reach the user's terminal, while stdout is captured and re-logged
    /// to stderr to keep machine output clean.
    #[allow(dead_code)]
    pub fn finish_conflict_op(&self) -> Result<FinishOutcome, GcmError> {
        // Dispatch order matters: a stopped rebase or cherry-pick can carry
        // auxiliary merge state, so MERGE_HEAD alone must not route to
        // `git commit`.
        let (op, args): (&'static str, &[&str]) = if self.is_rebasing() {
            (
                "rebase",
                &["-c", "commit.gpgsign=true", "rebase", "--continue"],
            )
        } else if self.is_cherry_picking() {
            (
                "cherry-pick",
                &["-c", "commit.gpgsign=true", "cherry-pick", "--continue"],
            )
        } else if self.is_merging() {
            ("merge", &["commit", "-S", "--no-edit"])
        } else {
            return Ok(FinishOutcome::NothingToFinish);
        };

        let output = self
            .git(args)
            .env("GIT_EDITOR", "true")
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git {op} finish: {e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            eprintln!("{stdout}");
        }

        // Classify strictly by postconditions - the exit code alone decides
        // nothing (a rebase stopping on the next conflicted commit also exits
        // non-zero, and hook stderr is unreliable to parse).
        let unmerged = self.unmerged_files()?;
        if !self.has_conflict_state() {
            if unmerged.is_empty() {
                let head_sha = self
                    .capture(&["rev-parse", "--short", "HEAD"])?
                    .trim()
                    .to_string();
                return Ok(FinishOutcome::Completed { head_sha });
            }
            return Ok(FinishOutcome::Failed { op });
        }
        if !unmerged.is_empty() && (op == "rebase" || op == "cherry-pick") {
            // The caller staged everything before this call, so any unmerged
            // entries now are NEW conflicts from the next commit in sequence.
            return Ok(FinishOutcome::StoppedOnNextConflict);
        }
        Ok(FinishOutcome::Failed { op })
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
        self.has_head_ref("MERGE_HEAD")
    }

    /// True if a rebase is in progress (`.git/REBASE_HEAD` exists).
    pub fn is_rebasing(&self) -> bool {
        self.has_head_ref("REBASE_HEAD")
    }

    /// True if a cherry-pick is in progress (`.git/CHERRY_PICK_HEAD` exists).
    pub fn is_cherry_picking(&self) -> bool {
        self.has_head_ref("CHERRY_PICK_HEAD")
    }

    /// True if any conflict-style operation is in progress (merge, rebase, or
    /// cherry-pick).
    pub fn has_conflict_state(&self) -> bool {
        self.is_merging() || self.is_rebasing() || self.is_cherry_picking()
    }

    fn has_head_ref(&self, name: &str) -> bool {
        self.git(&["rev-parse", "--verify", "--quiet", name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Enumerate unmerged (conflicted) file paths via `git diff --name-only --diff-filter=U -z`.
    /// NUL-delimited so unicode/space/newline paths survive.
    pub fn unmerged_files(&self) -> Result<Vec<String>, GcmError> {
        let out = self
            .git(&["diff", "--name-only", "--diff-filter=U", "-z"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git diff: {e}")))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git diff --name-only --diff-filter=U failed: {}",
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

    /// Re-checkout the given paths with zdiff3 conflict markers. Requires the
    /// caller to already be in a conflict state. Preserves the merge state; only
    /// the working-tree content is rewritten.
    pub fn checkout_conflict_zdiff3(&self, paths: &[&str]) -> Result<(), GcmError> {
        let mut cmd = self.git(&["checkout", "--conflict=zdiff3"]);
        cmd.env("GIT_LITERAL_PATHSPECS", "1");
        cmd.arg("--");
        cmd.args(paths);
        let out = cmd.output().map_err(|e| {
            GcmError::Git(format!("failed to run git checkout --conflict=zdiff3: {e}"))
        })?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git checkout --conflict=zdiff3 failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(())
    }

    /// Read a file's content from the working tree as a UTF-8 string.
    pub fn read_file(&self, path: &str) -> Result<String, GcmError> {
        let full = self.root.join(path);
        std::fs::read_to_string(&full)
            .map_err(|e| GcmError::Git(format!("could not read {}: {e}", full.display())))
    }

    /// Write content to a file in the working tree.
    pub fn write_file(&self, path: &str, content: &str) -> Result<(), GcmError> {
        let full = self.root.join(path);
        std::fs::write(&full, content)
            .map_err(|e| GcmError::Git(format!("could not write {}: {e}", full.display())))
    }

    /// Read a file's raw bytes from the working tree. Byte-exact counterpart of
    /// [`Self::read_file`] for snapshot/restore, where UTF-8 lossiness or
    /// line-ending normalization would corrupt the restored file.
    pub fn read_file_bytes(&self, path: &str) -> Result<Vec<u8>, GcmError> {
        let full = self.root.join(path);
        std::fs::read(&full)
            .map_err(|e| GcmError::Git(format!("could not read {}: {e}", full.display())))
    }

    /// Write raw bytes to a working-tree file (byte-exact restore IO).
    pub fn write_file_bytes(&self, path: &str, bytes: &[u8]) -> Result<(), GcmError> {
        let full = self.root.join(path);
        std::fs::write(&full, bytes)
            .map_err(|e| GcmError::Git(format!("could not write {}: {e}", full.display())))
    }

    /// Detect binary conflicted files from the combined unmerged diff. Binary
    /// files appear as `Binary files differ` under their `diff --cc <path>`
    /// header; text conflicts show hunk content instead. Returns the set of
    /// unmerged paths that are binary.
    pub fn binary_unmerged_files(&self) -> Result<Vec<String>, GcmError> {
        let out = self
            .git(&["diff", "--diff-filter=U"])
            .output()
            .map_err(|e| GcmError::Git(format!("failed to run git diff: {e}")))?;
        if !out.status.success() {
            return Err(GcmError::Git(format!(
                "git diff --diff-filter=U failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut binary = Vec::new();
        let mut current_path: Option<String> = None;
        let mut saw_binary = false;
        for line in text.lines() {
            if let Some(path) = line.strip_prefix("diff --cc ") {
                if let Some(p) = current_path.take().filter(|_| saw_binary) {
                    binary.push(p);
                }
                current_path = Some(path.to_string());
                saw_binary = false;
            } else if line == "Binary files differ" {
                saw_binary = true;
            } else if line.starts_with("@@@") || line.starts_with("++<<<<<<<") {
                // Hunk content for a text conflict: this file is not binary.
                saw_binary = false;
            }
        }
        if let Some(p) = current_path.take().filter(|_| saw_binary) {
            binary.push(p);
        }
        Ok(binary)
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
        self.stage_pathspecs(stdin_bytes)
    }

    /// Stage exact paths (resolve apply phase). Same literal, NUL-delimited
    /// pathspec mechanism as [`Self::stage_group`], so a filename containing a
    /// glob metacharacter can never pull in siblings.
    pub fn stage_paths(&self, paths: &[&str]) -> Result<(), GcmError> {
        let mut stdin_bytes: Vec<u8> = Vec::new();
        for p in paths {
            stdin_bytes.extend_from_slice(p.as_bytes());
            stdin_bytes.push(0);
        }
        self.stage_pathspecs(stdin_bytes)
    }

    fn stage_pathspecs(&self, stdin_bytes: Vec<u8>) -> Result<(), GcmError> {
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

    /// Whether the entry has staged (index-side) changes - a curated index gcm is
    /// about to reset/override (FR-46). Reads the index-side status char `x`: any
    /// real change (`M`/`A`/`D`/`R`/`C`...) except a clean index (` `) or an
    /// untracked entry (`?`). Unmerged entries can also satisfy this, but the
    /// caller checks `is_staged` only after the `is_unmerged` abort guard, so a
    /// conflict never reaches it.
    pub fn is_staged(&self) -> bool {
        self.x != b' ' && self.x != b'?'
    }

    /// Whether the entry is partially staged - both the index (`x`) and worktree
    /// (`y`) sides diverge (the `git add -p` / staged-then-modified signature,
    /// e.g. `MM`, `AM`, `MD`). This is the data-loss case v1 cannot preserve:
    /// gcm stages whole files, so the worktree hunks the user excluded get
    /// committed anyway (FR-46).
    pub fn is_partially_staged(&self) -> bool {
        self.is_staged() && self.y != b' ' && self.y != b'?'
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
    fn is_staged_reflects_index_side() {
        assert!(
            parse_status_z(b"M  a.txt\0")[0].is_staged(),
            "staged-only modify"
        );
        assert!(
            !parse_status_z(b" M a.txt\0")[0].is_staged(),
            "unstaged-only"
        );
        assert!(
            parse_status_z(b"MM a.txt\0")[0].is_staged(),
            "partially staged"
        );
        assert!(
            parse_status_z(b"A  a.txt\0")[0].is_staged(),
            "added (fully staged)"
        );
        assert!(!parse_status_z(b"?? a.txt\0")[0].is_staged(), "untracked");
        assert!(
            parse_status_z(b"R  new.txt\0old.txt\0")[0].is_staged(),
            "rename staged"
        );
    }

    #[test]
    fn is_partially_staged_requires_both_sides() {
        assert!(
            !parse_status_z(b"M  a.txt\0")[0].is_partially_staged(),
            "fully staged, no worktree delta"
        );
        assert!(
            !parse_status_z(b" M a.txt\0")[0].is_partially_staged(),
            "unstaged-only"
        );
        assert!(parse_status_z(b"MM a.txt\0")[0].is_partially_staged(), "MM");
        assert!(parse_status_z(b"AM a.txt\0")[0].is_partially_staged(), "AM");
        assert!(
            parse_status_z(b"MD a.txt\0")[0].is_partially_staged(),
            "staged mod + worktree delete"
        );
        assert!(
            !parse_status_z(b"A  a.txt\0")[0].is_partially_staged(),
            "added, no worktree delta"
        );
        assert!(
            !parse_status_z(b"?? a.txt\0")[0].is_partially_staged(),
            "untracked"
        );
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

    #[test]
    fn stage_group_stages_a_deletion() {
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("del.txt"), "bye").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "init"]);
        std::fs::remove_file(root.join("del.txt")).unwrap();

        let files = repo.changed_files().unwrap();
        repo.clear_staged().unwrap();
        repo.stage_group(&[cf_for(&files, "del.txt")]).unwrap();

        let ls = run_git(root, &["ls-files"]);
        assert!(
            !String::from_utf8_lossy(&ls.stdout).contains("del.txt"),
            "deletion is staged (file dropped from the index)"
        );
    }

    #[test]
    fn clear_staged_resets_index_to_head() {
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("f.txt"), "a").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "init"]);
        std::fs::write(root.join("f.txt"), "b").unwrap();
        run_git(root, &["add", "-A"]); // stage the modification
        assert!(
            !staged_names(root).is_empty(),
            "precondition: something staged"
        );

        repo.clear_staged().unwrap();
        assert!(staged_names(root).is_empty(), "index reset to HEAD");
    }

    #[test]
    fn changed_files_flags_a_merge_conflict() {
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("f.txt"), "base\n").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "base"]);
        let base = String::from_utf8_lossy(&run_git(root, &["branch", "--show-current"]).stdout)
            .trim()
            .to_string();
        run_git(root, &["switch", "-q", "-c", "feature"]);
        std::fs::write(root.join("f.txt"), "feature\n").unwrap();
        run_git(root, &["commit", "-qam", "feature"]);
        run_git(root, &["switch", "-q", &base]);
        std::fs::write(root.join("f.txt"), "mainline\n").unwrap();
        run_git(root, &["commit", "-qam", "mainline"]);
        let _ = run_git(root, &["merge", "feature"]); // expected to conflict

        let files = repo.changed_files().unwrap();
        assert!(
            files.iter().any(|c| c.is_unmerged()),
            "conflict surfaces as an unmerged entry"
        );
        assert!(repo.is_merging(), "MERGE_HEAD present during the conflict");
        assert!(
            repo.has_conflict_state(),
            "has_conflict_state true during merge"
        );

        // ST1: unmerged_files enumerates the conflicted path.
        let unmerged = repo.unmerged_files().unwrap();
        assert_eq!(unmerged, vec!["f.txt"]);

        // ST1: binary_unmerged_files returns empty for a text conflict.
        let binary = repo.binary_unmerged_files().unwrap();
        assert!(binary.is_empty(), "text conflict is not binary");

        // ST1: checkout_conflict_zdiff3 rewrites the file with markers without
        // clearing MERGE_HEAD.
        repo.checkout_conflict_zdiff3(&["f.txt"]).unwrap();
        assert!(
            repo.is_merging(),
            "merge state preserved after zdiff3 re-checkout"
        );
        let content = repo.read_file("f.txt").unwrap();
        assert!(content.contains("<<<<<<<"), "zdiff3 markers present");
        assert!(content.contains("|||||||"), "zdiff3 base block present");
        assert!(content.contains("======="), "zdiff3 separator present");
        assert!(content.contains(">>>>>>>"), "zdiff3 end marker present");
    }

    #[test]
    fn unmerged_files_nul_delimited() {
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("base.txt"), "base\n").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "base"]);
        let base = String::from_utf8_lossy(&run_git(root, &["branch", "--show-current"]).stdout)
            .trim()
            .to_string();
        run_git(root, &["switch", "-q", "-c", "feature"]);
        std::fs::write(root.join("file with spaces.txt"), "feature\n").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-qam", "feature"]);
        run_git(root, &["switch", "-q", &base]);
        std::fs::write(root.join("file with spaces.txt"), "mainline\n").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-qam", "mainline"]);
        let _ = run_git(root, &["merge", "feature"]);

        let unmerged = repo.unmerged_files().unwrap();
        assert_eq!(unmerged, vec!["file with spaces.txt"]);
    }

    /// Probe whether `git commit -S` works in this environment (mirrors
    /// `scripts/acceptance.sh` `probe_signing`). CI runners have no signing
    /// key, so signing-dependent tests skip there and run on dev machines.
    fn signing_available() -> bool {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        run_git(root, &["init", "-q"]);
        run_git(root, &["config", "user.email", "t@t"]);
        run_git(root, &["config", "user.name", "T"]);
        run_git(
            root,
            &["commit", "-S", "--allow-empty", "-q", "-m", "probe"],
        )
        .status
        .success()
    }

    /// Build a merge stopped on a conflict in `f.txt`, with the resolution
    /// already written and staged (the state `finish_conflict_op` expects).
    fn staged_conflicted_merge(root: &Path) {
        std::fs::write(root.join("f.txt"), "base\n").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "base"]);
        run_git(root, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(root.join("f.txt"), "feature\n").unwrap();
        run_git(root, &["commit", "-q", "-am", "feature"]);
        run_git(root, &["checkout", "-q", "-"]);
        std::fs::write(root.join("f.txt"), "main side\n").unwrap();
        run_git(root, &["commit", "-q", "-am", "main side"]);
        run_git(root, &["merge", "feature"]); // exits non-zero: conflict
        std::fs::write(root.join("f.txt"), "resolved\n").unwrap();
        run_git(root, &["add", "f.txt"]);
    }

    #[test]
    fn finish_nothing_to_finish_without_conflict_state() {
        let (dir, repo) = temp_repo();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        run_git(dir.path(), &["add", "-A"]);
        run_git(dir.path(), &["commit", "-q", "-m", "init"]);
        assert_eq!(
            repo.finish_conflict_op().unwrap(),
            FinishOutcome::NothingToFinish
        );
    }

    #[test]
    fn finish_merge_completes_with_signed_two_parent_commit() {
        if !signing_available() {
            eprintln!("skipping finish_merge_completes: commit signing unavailable here");
            return;
        }
        let (dir, repo) = temp_repo();
        staged_conflicted_merge(dir.path());
        assert!(repo.is_merging(), "precondition: merge in progress");

        let outcome = repo.finish_conflict_op().unwrap();
        let FinishOutcome::Completed { head_sha } = outcome else {
            panic!("expected Completed, got {outcome:?}");
        };
        assert!(!head_sha.is_empty());
        assert!(!repo.is_merging(), "MERGE_HEAD cleared");
        assert!(repo.unmerged_files().unwrap().is_empty());
        // Two parents = a real merge commit.
        let second_parent = run_git(dir.path(), &["rev-parse", "--verify", "HEAD^2"]);
        assert!(second_parent.status.success(), "HEAD has a second parent");
        // Signature header present (gpgsig covers both GPG and SSH signing).
        let raw = run_git(dir.path(), &["cat-file", "commit", "HEAD"]);
        assert!(
            String::from_utf8_lossy(&raw.stdout).contains("gpgsig"),
            "merge commit carries a signature header"
        );
    }

    #[test]
    fn finish_merge_hook_rejection_keeps_staged_state() {
        let (dir, repo) = temp_repo();
        staged_conflicted_merge(dir.path());
        let hook_dir = dir.path().join(".git/hooks");
        std::fs::create_dir_all(&hook_dir).unwrap();
        let hook = hook_dir.join("pre-commit");
        std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // Fails on the hook (before signing), so this runs on unsigned machines too.
        let outcome = repo.finish_conflict_op().unwrap();
        assert_eq!(outcome, FinishOutcome::Failed { op: "merge" });
        assert!(repo.is_merging(), "MERGE_HEAD preserved for manual retry");
        assert!(
            staged_names(dir.path()).contains(&"f.txt".to_string()),
            "staged resolution untouched"
        );
    }

    #[test]
    fn finish_cherry_pick_completes_and_clears_state() {
        if !signing_available() {
            eprintln!("skipping finish_cherry_pick: commit signing unavailable here");
            return;
        }
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("f.txt"), "base\n").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "base"]);
        run_git(root, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(root.join("f.txt"), "feature\n").unwrap();
        run_git(root, &["commit", "-q", "-am", "feature"]);
        run_git(root, &["checkout", "-q", "-"]);
        std::fs::write(root.join("f.txt"), "main side\n").unwrap();
        run_git(root, &["commit", "-q", "-am", "main side"]);
        run_git(root, &["cherry-pick", "feature"]); // conflict
        assert!(
            repo.is_cherry_picking(),
            "precondition: cherry-pick stopped"
        );
        std::fs::write(root.join("f.txt"), "resolved\n").unwrap();
        run_git(root, &["add", "f.txt"]);

        let outcome = repo.finish_conflict_op().unwrap();
        assert!(
            matches!(outcome, FinishOutcome::Completed { .. }),
            "expected Completed, got {outcome:?}"
        );
        assert!(!repo.is_cherry_picking(), "CHERRY_PICK_HEAD cleared");
    }

    #[test]
    fn finish_rebase_stops_on_next_conflicted_commit() {
        if !signing_available() {
            eprintln!("skipping finish_rebase_stops: commit signing unavailable here");
            return;
        }
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("f.txt"), "base\n").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "base"]);
        run_git(root, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(root.join("f.txt"), "f1\n").unwrap();
        run_git(root, &["commit", "-q", "-am", "c1"]);
        std::fs::write(root.join("f.txt"), "f2\n").unwrap();
        run_git(root, &["commit", "-q", "-am", "c2"]);
        run_git(root, &["checkout", "-q", "-"]);
        std::fs::write(root.join("f.txt"), "moved\n").unwrap();
        run_git(root, &["commit", "-q", "-am", "move main"]);
        run_git(root, &["checkout", "-q", "feature"]);
        run_git(root, &["rebase", "@{-1}"]); // stops on c1
        assert!(repo.is_rebasing(), "precondition: rebase stopped on c1");
        std::fs::write(root.join("f.txt"), "r1\n").unwrap();
        run_git(root, &["add", "f.txt"]);

        // Continuing commits r1 (signed), then c2 conflicts against it.
        let outcome = repo.finish_conflict_op().unwrap();
        assert_eq!(outcome, FinishOutcome::StoppedOnNextConflict);
        assert!(repo.is_rebasing(), "rebase still in progress at next stop");
        assert!(
            !repo.unmerged_files().unwrap().is_empty(),
            "next commit's conflict is present"
        );
    }

    #[test]
    fn file_bytes_round_trip_is_byte_exact() {
        let (dir, repo) = temp_repo();
        let root = dir.path();
        // Non-UTF8 bytes: the String-based read_file would reject or mangle these.
        let bytes: Vec<u8> = vec![0xff, 0x00, b'\r', b'\n', 0xfe, b'x'];
        std::fs::write(root.join("bin.dat"), &bytes).unwrap();
        assert_eq!(repo.read_file_bytes("bin.dat").unwrap(), bytes);
        repo.write_file_bytes("bin.dat", &bytes).unwrap();
        assert_eq!(std::fs::read(root.join("bin.dat")).unwrap(), bytes);
    }

    #[test]
    fn write_file_persists_resolution() {
        let (dir, repo) = temp_repo();
        let root = dir.path();
        std::fs::write(root.join("f.txt"), "a").unwrap();
        repo.write_file("f.txt", "resolved content").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("f.txt")).unwrap(),
            "resolved content"
        );
    }

    #[test]
    fn no_conflict_state_without_merge() {
        let (_dir, repo) = temp_repo();
        assert!(!repo.is_merging());
        assert!(!repo.is_rebasing());
        assert!(!repo.is_cherry_picking());
        assert!(!repo.has_conflict_state());
        assert!(repo.unmerged_files().unwrap().is_empty());
    }

    #[test]
    fn binary_unmerged_file_detected() {
        let (dir, repo) = temp_repo();
        let root = dir.path();
        // Mark *.bin as binary so git's diff --numstat shows `-\t-`.
        std::fs::write(root.join(".gitattributes"), "*.bin binary\n").unwrap();
        std::fs::write(
            root.join("img.bin"),
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00",
        )
        .unwrap();
        std::fs::write(root.join("f.txt"), "base\n").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-q", "-m", "base"]);
        let base = String::from_utf8_lossy(&run_git(root, &["branch", "--show-current"]).stdout)
            .trim()
            .to_string();
        run_git(root, &["switch", "-q", "-c", "feature"]);
        std::fs::write(root.join("f.txt"), "feature\n").unwrap();
        std::fs::write(root.join("img.bin"), b"\x89PNG\r\n\x1a\nCHANGED").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-qam", "feature"]);
        run_git(root, &["switch", "-q", &base]);
        std::fs::write(root.join("f.txt"), "mainline\n").unwrap();
        std::fs::write(root.join("img.bin"), b"\x89PNG\r\n\x1a\nMAINLINE").unwrap();
        run_git(root, &["add", "-A"]);
        run_git(root, &["commit", "-qam", "mainline"]);
        let _ = run_git(root, &["merge", "feature"]);

        let binary = repo.binary_unmerged_files().unwrap();
        assert!(
            binary.contains(&"img.bin".to_string()),
            "PNG conflict detected as binary"
        );
        assert!(
            !binary.contains(&"f.txt".to_string()),
            "text conflict is not binary"
        );
        // .gitattributes itself is also conflicted (modified on both sides); it is text.
        assert!(!binary.contains(&".gitattributes".to_string()));
    }
}
