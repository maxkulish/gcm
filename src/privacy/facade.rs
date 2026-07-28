use std::path::Path;

use crate::diff::{GatheredDiff, GroupingContext};
use crate::error::GcmError;
use crate::git::{ChangedFile, Repo};
use gcm::privacy::{Scanner, SecretScanMode};

pub struct Privacy {
    filter: PathFilter,
    scanner: Scanner<'static>,
}

impl Privacy {
    pub fn load(repo: &Repo, cli_secret_scan: Option<SecretScanMode>) -> Result<Self, GcmError> {
        let mode = SecretScanMode::resolve_with(cli_secret_scan, |key| std::env::var(key).ok())
            .map_err(GcmError::from)?;
        let scanner = Scanner::vendored(mode).map_err(GcmError::from)?;

        Ok(Self {
            filter: PathFilter::load(repo.root())?,
            scanner,
        })
    }

    pub fn filter_changed(&self, changed: &[ChangedFile]) -> Vec<ChangedFile> {
        self.filter.filter_changed(changed)
    }

    /// The active secret-scan mode (CLO-531).
    pub fn secret_scan_mode(&self) -> SecretScanMode {
        self.scanner.mode()
    }

    pub fn prepare_grouping(&self, ctx: GroupingContext) -> Result<GroupingContext, GcmError> {
        Ok(GroupingContext {
            file_list: self.scan_text(ctx.file_list)?,
            status: self.scan_text(ctx.status)?,
            stat: self.scan_text(ctx.stat)?,
            body: self.scan_text(ctx.body)?,
        })
    }

    pub fn prepare_diff(&self, diff: GatheredDiff) -> Result<GatheredDiff, GcmError> {
        Ok(GatheredDiff {
            stat: self.scan_text(diff.stat)?,
            body: self.scan_text(diff.body)?,
        })
    }

    /// Scan arbitrary text with the configured secret-scan mode. Public so the
    /// `gcm resolve` path can check hunk text before provider egress (CLO-531).
    pub fn scan_text(&self, text: String) -> Result<String, GcmError> {
        self.scanner.scan(text).map_err(GcmError::from)
    }
}

#[derive(Debug)]
struct PathFilter {
    patterns: Vec<IgnorePattern>,
}

impl PathFilter {
    fn load(repo_root: &Path) -> Result<Self, GcmError> {
        let mut patterns = vec![
            IgnorePattern::new(".gcmignore"),
            IgnorePattern::new("gcmignore"),
        ];

        for name in [".gcmignore", "gcmignore"] {
            let path = repo_root.join(name);
            if !path.exists() {
                continue;
            }
            let contents = std::fs::read_to_string(&path)
                .map_err(|e| GcmError::Config(format!("failed to read {}: {e}", path.display())))?;
            patterns.extend(contents.lines().filter_map(IgnorePattern::parse));
        }

        Ok(Self { patterns })
    }

    fn filter_changed(&self, changed: &[ChangedFile]) -> Vec<ChangedFile> {
        changed
            .iter()
            .filter(|c| {
                !self.matches(&c.path)
                    && match c.orig_path.as_deref() {
                        Some(orig) => !self.matches(orig),
                        None => true,
                    }
            })
            .cloned()
            .collect()
    }

    fn matches(&self, path: &str) -> bool {
        self.patterns.iter().any(|p| p.matches(path))
    }
}

#[derive(Debug)]
struct IgnorePattern {
    raw: String,
    dir_only: bool,
    basename_only: bool,
}

impl IgnorePattern {
    fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            return None;
        }
        Some(Self::new(trimmed))
    }

    fn new(raw: &str) -> Self {
        let mut pat = raw.trim().replace('\\', "/");
        while let Some(stripped) = pat.strip_prefix("./") {
            pat = stripped.to_string();
        }
        while let Some(stripped) = pat.strip_prefix('/') {
            pat = stripped.to_string();
        }
        let dir_only = pat.ends_with('/');
        if dir_only {
            pat.pop();
        }
        let basename_only = !pat.contains('/');
        Self {
            raw: pat,
            dir_only,
            basename_only,
        }
    }

    fn matches(&self, path: &str) -> bool {
        let normalized = normalize_path(path);
        if self.raw.is_empty() {
            return false;
        }

        if self.dir_only {
            return if self.basename_only {
                normalized
                    .split('/')
                    .any(|seg| wildcard_match(&self.raw, seg))
            } else {
                normalized == self.raw || normalized.starts_with(&format!("{}/", self.raw))
            };
        }

        if self.basename_only {
            let basename = normalized.rsplit('/').next().unwrap_or(&normalized);
            wildcard_match(&self.raw, basename)
        } else {
            wildcard_match(&self.raw, &normalized)
        }
    }
}

fn normalize_path(path: &str) -> String {
    let mut p = path.replace('\\', "/");
    while let Some(stripped) = p.strip_prefix("./") {
        p = stripped.to_string();
    }
    p
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; txt.len() + 1]; pat.len() + 1];
    dp[0][0] = true;

    for i in 1..=pat.len() {
        if pat[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=pat.len() {
        for j in 1..=txt.len() {
            dp[i][j] = match pat[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                c => c == txt[j - 1] && dp[i - 1][j - 1],
            };
        }
    }

    dp[pat.len()][txt.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed(path: &str) -> ChangedFile {
        ChangedFile {
            x: b' ',
            y: b'M',
            path: path.to_string(),
            orig_path: None,
        }
    }

    #[test]
    fn wildcard_matches_basename_and_paths() {
        assert!(IgnorePattern::new("*.pem").matches("secrets/key.pem"));
        assert!(IgnorePattern::new("secrets/*").matches("secrets/api.txt"));
        assert!(!IgnorePattern::new("secrets/*").matches("src/secrets/api.txt"));
        assert!(IgnorePattern::new("target/").matches("nested/target/file"));
    }

    #[test]
    fn filter_excludes_builtin_and_original_rename_path() {
        let filter = PathFilter {
            patterns: vec![
                IgnorePattern::new(".gcmignore"),
                IgnorePattern::new("secrets/*"),
            ],
        };
        let files = vec![
            changed(".gcmignore"),
            changed("src/lib.rs"),
            ChangedFile {
                x: b'R',
                y: b' ',
                path: "public.txt".to_string(),
                orig_path: Some("secrets/old.txt".to_string()),
            },
        ];
        let kept = filter.filter_changed(&files);
        assert_eq!(kept, vec![changed("src/lib.rs")]);
    }

    #[test]
    fn abort_mode_rejects_secret_text() {
        let scanner = Scanner::new(
            SecretScanMode::Abort,
            gcm::privacy::rules::vendored().unwrap(),
        );
        let privacy = Privacy {
            filter: PathFilter {
                patterns: vec![
                    IgnorePattern::new(".gcmignore"),
                    IgnorePattern::new("secrets/*"),
                ],
            },
            scanner,
        };
        let result = privacy.prepare_diff(GatheredDiff {
            stat: String::new(),
            body: "API_KEY=supersecret12345".to_string(),
        });
        assert!(matches!(result, Err(GcmError::SecretDetected { count: 1 })));
    }
}
