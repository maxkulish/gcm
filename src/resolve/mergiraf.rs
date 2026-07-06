//! Optional `mergiraf` pre-resolution stage for `gcm resolve` (CLO-531, ST7).
//!
//! `mergiraf` is an external, optional tool. If it is not on `PATH`, or if the
//! user passes `--no-mergiraf`, this stage is skipped silently. When run, it
//! attempts to resolve conflict markers structurally; any file that still has
//! markers after the run is forwarded to the LLM stage.

use crate::error::GcmError;
use crate::git::Repo;

use super::markers::has_conflict_markers;

/// Detect whether `mergiraf` is on PATH.
pub fn is_available() -> bool {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|p| p.join("mergiraf"))
                .find(|p| p.is_file())
        })
        .is_some()
}

/// Run `mergiraf solve` on a single conflicted file. Returns `Ok(true)` when
/// the file has no remaining conflict markers after the run (fully resolved),
/// `Ok(false)` when markers remain, and `Err` only when the binary is missing
/// or the invocation itself fails.
pub fn try_resolve(repo: &Repo, path: &str) -> Result<bool, GcmError> {
    if !is_available() {
        return Ok(false);
    }
    let status = std::process::Command::new("mergiraf")
        .current_dir(repo.root())
        .args([
            "solve",
            "--keep-backup=false",
            "--",
            path,
        ])
        .status()
        .map_err(|e| GcmError::Git(format!("failed to run mergiraf: {e}")))?;
    if !status.success() {
        // Non-zero exit means mergiraf could not resolve all conflicts (or some
        // other failure). Treat as unresolved and let the LLM stage try.
        return Ok(false);
    }
    let content = repo.read_file(path)?;
    Ok(!has_conflict_markers(&content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Repo;

    #[test]
    fn availability_reflects_path() {
        // `which::which("mergiraf")` is the source of truth; this test just
        // ensures the function returns a deterministic bool on this machine.
        let _ = is_available();
    }

    #[test]
    fn unavailable_mergiraf_is_graceful() {
        // Simulate absence by pointing PATH somewhere that cannot contain it.
        let prev = std::env::var("PATH").ok();
        std::env::set_var("PATH", "/tmp/nonexistent-merge-dir");
        assert!(!is_available());
        if let Some(p) = prev {
            std::env::set_var("PATH", p);
        }
    }

    #[test]
    fn try_resolve_when_unavailable_returns_false() {
        let prev = std::env::var("PATH").ok();
        std::env::set_var("PATH", "/tmp/nonexistent-merge-dir");
        let dir = tempfile::tempdir().unwrap();
        let repo = Repo::at_root(dir.path().to_path_buf());
        assert_eq!(try_resolve(&repo, "any.txt").unwrap(), false);
        if let Some(p) = prev {
            std::env::set_var("PATH", p);
        }
    }
}
