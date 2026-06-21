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
/// Folded into the fingerprint: bump when the grouping prompt or schema changes,
/// or when the fingerprint composition changes, so a cached plan from an older
/// contract re-analyzes. Bumped to 2 in CLO-489 (the provider is now folded in
/// via the provider-qualified model id instead of a hardcoded `groq` token).
const FINGERPRINT_VERSION: u32 = 2;

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
///
/// `pending` is the live change set the caller already computed (nothing has
/// mutated the tree since), so this does not re-run `git status`.
pub fn load(repo: &Repo, pending: &[ChangedFile], model: &str) -> Option<Plan> {
    let path = cache_path(repo.root())?;
    let data = fs::read(&path).ok()?;
    let cf = read_cache_file(&data)?;
    if fingerprint(repo, pending, model) != cf.fingerprint {
        return None;
    }
    Some(cf.plan)
}

/// Persist the full plan with a fresh fingerprint over the current pending set
/// (the caller's already-computed `pending`). Best-effort: a failure warns and
/// returns (the caller's commit still proceeds).
pub fn save(repo: &Repo, plan: &Plan, pending: &[ChangedFile], model: &str) {
    if let Err(e) = persist(repo, plan, pending, model) {
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

fn persist(repo: &Repo, plan: &Plan, pending: &[ChangedFile], model: &str) -> io::Result<()> {
    let path = cache_path(repo.root()).ok_or_else(no_cache_dir)?;
    let cf = CacheFile {
        version: CACHE_FORMAT_VERSION,
        fingerprint: fingerprint(repo, pending, model),
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

/// Fingerprint over the pending change set (FR-27): version + provider-qualified
/// model id (e.g. "groq:openai/gpt-oss-120b", so a provider OR model switch
/// re-analyzes) + per-file (path, content hash), with paths sorted for
/// stability. Read from the LIVE change set each run; never pins `HEAD`;
/// unborn-safe (working-tree reads + `git status` only). The cache KEY/location
/// (`sha256(repo-root)`) is provider-independent (FR-25); only this freshness
/// fingerprint gains provider awareness (CLO-489).
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
        // The file existed at symlink_metadata, so an open failure here is a
        // permission/IO error, NOT a deletion - mark it unreadable (a deletion
        // is only the symlink_metadata-absent case above).
        Err(_) => return "\0UNREADABLE".to_string(),
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

    fn group(files: &[&str], msg: Option<&str>) -> Group {
        Group {
            files: files.iter().map(|s| s.to_string()).collect(),
            summary: "s".to_string(),
            commit_message: msg.map(|m| m.to_string()),
        }
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

    #[test]
    fn hash_path_hashes_regular_file_content() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.txt");
        std::fs::write(&p, b"hello").unwrap();
        let h = hash_path(&p);
        assert_eq!(h.len(), 64, "regular file -> hex sha256");
        let p2 = dir.path().join("g.txt");
        std::fs::write(&p2, b"hello").unwrap();
        assert_eq!(h, hash_path(&p2), "same content -> same hash");
        std::fs::write(&p2, b"world").unwrap();
        assert_ne!(h, hash_path(&p2), "different content -> different hash");
    }

    #[test]
    fn hash_path_missing_file_is_deleted_marker() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(hash_path(&dir.path().join("nope")), "\0DELETED");
    }

    #[cfg(unix)]
    #[test]
    fn hash_path_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.txt");
        std::fs::write(&target, b"secret-bytes").unwrap();
        let link = dir.path().join("link.txt");
        symlink(&target, &link).unwrap();
        // A symlink is hashed by its target PATH, not the pointed-to bytes - so
        // it must differ from the regular-file hash of that content (proving the
        // link was not followed: no FIFO-block, no out-of-repo content leak).
        assert_ne!(
            hash_path(&link),
            hash_path(&target),
            "symlink must not be followed into its target's content"
        );
        let link2 = dir.path().join("link2.txt");
        symlink(&target, &link2).unwrap();
        assert_eq!(
            hash_path(&link),
            hash_path(&link2),
            "same target path -> same symlink hash"
        );
    }

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
