//! XDG-style base-directory resolution.
//!
//! The `directories` crate maps to macOS conventions (`~/Library/Application
//! Support/gcm`, with a space; `~/Library/Caches/gcm`). gcm instead uses the XDG
//! layout on every platform - `~/.config/gcm` and `~/.cache/gcm` - so its files
//! live where CLI tools conventionally keep them and are easy to type.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// `gcm`'s subdirectory under an XDG base dir, resolved purely from env values so
/// the precedence is unit-testable without touching the process environment.
///
/// `xdg_home` (e.g. `$XDG_CONFIG_HOME`) wins when set to a non-empty **absolute**
/// path - per the XDG Base Directory spec, relative values are ignored. Otherwise
/// `<home>/<default_rel>` (e.g. `~/.config`). Returns `None` when neither yields a
/// usable absolute base (a headless environment with no `HOME` and no override).
pub fn xdg_gcm_dir_from(
    xdg_home: Option<&OsStr>,
    home: Option<&OsStr>,
    default_rel: &str,
) -> Option<PathBuf> {
    if let Some(x) = xdg_home {
        let p = Path::new(x);
        if p.is_absolute() {
            return Some(p.join("gcm"));
        }
    }
    home.map(Path::new)
        .filter(|h| h.is_absolute())
        .map(|h| h.join(default_rel).join("gcm"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_xdg_home_wins() {
        let p = xdg_gcm_dir_from(
            Some(OsStr::new("/xdg/config")),
            Some(OsStr::new("/home/u")),
            ".config",
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/xdg/config/gcm"));
    }

    #[test]
    fn empty_xdg_home_falls_back_to_home() {
        let p =
            xdg_gcm_dir_from(Some(OsStr::new("")), Some(OsStr::new("/home/u")), ".config").unwrap();
        assert_eq!(p, PathBuf::from("/home/u/.config/gcm"));
    }

    #[test]
    fn relative_xdg_home_is_ignored() {
        // Per the XDG spec a relative XDG_* value must be ignored.
        let p = xdg_gcm_dir_from(
            Some(OsStr::new("relative/config")),
            Some(OsStr::new("/home/u")),
            ".config",
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("/home/u/.config/gcm"));
    }

    #[test]
    fn home_only_uses_default_rel() {
        let p = xdg_gcm_dir_from(None, Some(OsStr::new("/home/u")), ".config").unwrap();
        assert_eq!(p, PathBuf::from("/home/u/.config/gcm"));
        let c = xdg_gcm_dir_from(None, Some(OsStr::new("/home/u")), ".cache").unwrap();
        assert_eq!(c, PathBuf::from("/home/u/.cache/gcm"));
    }

    #[test]
    fn no_base_at_all_is_none() {
        assert!(xdg_gcm_dir_from(None, None, ".config").is_none());
        // a relative HOME is not usable either
        assert!(xdg_gcm_dir_from(None, Some(OsStr::new("relative")), ".config").is_none());
    }
}
