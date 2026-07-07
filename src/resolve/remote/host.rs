//! Host detection for remote MR/PR resolution (CLO-533).
//!
//! Supports GitHub and GitLab, including self-hosted instances detected by
//! domain heuristic. All detection is done from the user's existing remotes or
//! from a full URL; no network calls.

use crate::error::GcmError;
use crate::git::Repo;

/// Supported remote hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    GitHub,
    GitLab,
}

impl Host {
    /// The external CLI binary used to interact with this host.
    pub fn cli_name(self) -> &'static str {
        match self {
            Host::GitHub => "gh",
            Host::GitLab => "glab",
        }
    }

    /// Install/auth hint shown when the CLI is missing or unauthenticated.
    pub fn install_hint(self) -> &'static str {
        match self {
            Host::GitHub => "install `gh` and run `gh auth login` (scopes: repo, read:org)",
            Host::GitLab => "install `glab` and run `glab auth login` (scopes: api, read_user)",
        }
    }

    /// Return the host family token for the deterministic resolution branch.
    pub fn resolution_slug(self) -> &'static str {
        match self {
            Host::GitHub => "github",
            Host::GitLab => "gitlab",
        }
    }
}

/// Parsed reference to a remote PR/MR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRef {
    pub host: Host,
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

/// Resolve a `--pr` or `--mr` argument into a `RemoteRef`.
///
/// * `arg` is either a full URL (`https://github.com/owner/repo/pull/42`) or a
///   bare numeric id. When `arg` is bare, the current repo's `origin` remote is
///   consulted to determine owner/repo and host.
/// * `preferred_host` is `Some(Host)` when the user passed `--pr` or `--mr` and
///   helps disambiguate bare ids or URLs without an explicit host token.
pub fn resolve_remote_ref(
    arg: &str,
    preferred_host: Option<Host>,
    current_repo: Option<&Repo>,
) -> Result<RemoteRef, GcmError> {
    let trimmed = arg.trim();

    // Full URL path.
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("git@")
    {
        return parse_url(trimmed, preferred_host);
    }

    // Bare numeric id.
    let number = trimmed.parse::<u64>().map_err(|_| GcmError::RemoteHost {
        host: trimmed.to_string(),
        reason: "expected a full URL or a numeric PR/MR id".to_string(),
    })?;

    let origin = current_repo
        .ok_or_else(|| GcmError::RemoteHost {
            host: "unknown".to_string(),
            reason: "numeric id requires a git repository with an origin remote; pass a full URL instead".to_string(),
        })?
        .remote_url("origin")
        .map_err(|e| GcmError::Git(e.to_string()))?
        .ok_or_else(|| GcmError::RemoteHost {
            host: "unknown".to_string(),
            reason: "no origin remote found; pass a full URL instead".to_string(),
        })?;

    let parsed = parse_origin_url(&origin, preferred_host)?;
    Ok(RemoteRef {
        host: parsed.host,
        owner: parsed.owner,
        repo: parsed.repo,
        number,
    })
}

fn parse_url(url: &str, preferred_host: Option<Host>) -> Result<RemoteRef, GcmError> {
    // Normalize git@host:owner/repo.git into https://host/owner/repo.git for parsing.
    let normalized = if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':').ok_or_else(|| GcmError::RemoteHost {
            host: url.to_string(),
            reason: "SSH URL must use host:path format".to_string(),
        })?;
        format!("https://{host}/{path}")
    } else {
        url.to_string()
    };

    let parsed = url::Url::parse(&normalized).map_err(|_| GcmError::RemoteHost {
        host: url.to_string(),
        reason: "could not parse URL".to_string(),
    })?;

    let host = detect_host(&parsed, preferred_host)?;
    let mut path_segments: Vec<&str> = parsed
        .path_segments()
        .map(|s| s.collect())
        .unwrap_or_default();

    // Drop trailing ".git" from repo segment if present.
    if let Some(last) = path_segments.last_mut() {
        *last = last.strip_suffix(".git").unwrap_or(*last);
    }

    // GitHub: /owner/repo/pull/42
    // GitLab: /owner/repo/-/merge_requests/42
    let number = if host == Host::GitHub {
        extract_number(&path_segments, "pull")?
    } else {
        extract_number_gitlab(&path_segments)?
    };

    if path_segments.len() < 2 {
        return Err(GcmError::RemoteHost {
            host: parsed.host_str().unwrap_or("").to_string(),
            reason: "URL does not contain owner/repo".to_string(),
        });
    }

    Ok(RemoteRef {
        host,
        owner: path_segments[0].to_string(),
        repo: path_segments[1].to_string(),
        number,
    })
}

fn parse_origin_url(url: &str, preferred_host: Option<Host>) -> Result<RemoteRef, GcmError> {
    // Same normalization logic as parse_url but we don't need a PR number.
    let normalized = if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':').ok_or_else(|| GcmError::RemoteHost {
            host: url.to_string(),
            reason: "SSH origin must use host:path format".to_string(),
        })?;
        format!("https://{host}/{path}")
    } else {
        url.to_string()
    };

    let parsed = url::Url::parse(&normalized).map_err(|_| GcmError::RemoteHost {
        host: url.to_string(),
        reason: "could not parse origin remote URL".to_string(),
    })?;

    let host = detect_host(&parsed, preferred_host)?;
    let mut path_segments: Vec<&str> = parsed
        .path_segments()
        .map(|s| s.collect())
        .unwrap_or_default();

    if let Some(last) = path_segments.last_mut() {
        *last = last.strip_suffix(".git").unwrap_or(*last);
    }

    if path_segments.len() < 2 {
        return Err(GcmError::RemoteHost {
            host: parsed.host_str().unwrap_or("").to_string(),
            reason: "origin remote URL does not contain owner/repo".to_string(),
        });
    }

    Ok(RemoteRef {
        host,
        owner: path_segments[0].to_string(),
        repo: path_segments[1].to_string(),
        number: 0,
    })
}

fn detect_host(url: &url::Url, preferred_host: Option<Host>) -> Result<Host, GcmError> {
    let host_str = url.host_str().unwrap_or("").to_lowercase();

    if host_str == "github.com" || host_str.ends_with(".github.com") {
        return Ok(Host::GitHub);
    }
    if host_str == "gitlab.com" || host_str.ends_with(".gitlab.com") {
        return Ok(Host::GitLab);
    }

    // Domain heuristic for self-hosted instances. Require the user to
    // disambiguate unknown hosts with --pr/--mr.
    if let Some(h) = preferred_host {
        return Ok(h);
    }

    Err(GcmError::RemoteHost {
        host: url.host_str().unwrap_or("").to_string(),
        reason: "could not detect host from URL; pass a full github.com/gitlab.com URL or use a recognizable self-hosted domain".to_string(),
    })
}

fn extract_number(segments: &[&str], keyword: &str) -> Result<u64, GcmError> {
    for (i, seg) in segments.iter().enumerate() {
        if seg.eq_ignore_ascii_case(keyword) {
            let num_seg = segments.get(i + 1).ok_or_else(|| GcmError::RemoteHost {
                host: "github".to_string(),
                reason: format!("URL ends after /{keyword}/; expected a number"),
            })?;
            return num_seg.parse::<u64>().map_err(|_| GcmError::RemoteHost {
                host: "github".to_string(),
                reason: format!("/{keyword}/{num_seg} is not a valid number"),
            });
        }
    }
    Err(GcmError::RemoteHost {
        host: "github".to_string(),
        reason: "GitHub URL must contain /pull/<number>".to_string(),
    })
}

fn extract_number_gitlab(segments: &[&str]) -> Result<u64, GcmError> {
    for (i, seg) in segments.iter().enumerate() {
        if seg.eq_ignore_ascii_case("merge_requests") {
            let num_seg = segments.get(i + 1).ok_or_else(|| GcmError::RemoteHost {
                host: "gitlab".to_string(),
                reason: "GitLab URL ends after /merge_requests/; expected a number".to_string(),
            })?;
            return num_seg.parse::<u64>().map_err(|_| GcmError::RemoteHost {
                host: "gitlab".to_string(),
                reason: format!("/merge_requests/{num_seg} is not a valid number"),
            });
        }
    }
    Err(GcmError::RemoteHost {
        host: "gitlab".to_string(),
        reason: "GitLab URL must contain /-/merge_requests/<number>".to_string(),
    })
}

/// Verify the required host CLI is on PATH; otherwise return an actionable error.
pub fn require_host_cli(host: Host) -> Result<(), GcmError> {
    let cli = host.cli_name();
    if which::which(cli).is_ok() {
        return Ok(());
    }
    Err(GcmError::RemoteCliMissing {
        cli: cli.to_string(),
        install_hint: host.install_hint().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_url_https() {
        let r = resolve_remote_ref(
            "https://github.com/acme/app/pull/42",
            Some(Host::GitHub),
            None,
        )
        .unwrap();
        assert_eq!(r.host, Host::GitHub);
        assert_eq!(r.owner, "acme");
        assert_eq!(r.repo, "app");
        assert_eq!(r.number, 42);
    }

    #[test]
    fn parse_github_url_ssh() {
        // A bare SSH repo URL is used only for origin lookup, not PR number extraction.
        let r = parse_origin_url("git@github.com:acme/app.git", Some(Host::GitHub)).unwrap();
        assert_eq!(r.host, Host::GitHub);
        assert_eq!(r.owner, "acme");
        assert_eq!(r.repo, "app");
        assert_eq!(r.number, 0);
    }

    #[test]
    fn parse_gitlab_url_https() {
        let r = resolve_remote_ref(
            "https://gitlab.com/acme/app/-/merge_requests/7",
            Some(Host::GitLab),
            None,
        )
        .unwrap();
        assert_eq!(r.host, Host::GitLab);
        assert_eq!(r.owner, "acme");
        assert_eq!(r.repo, "app");
        assert_eq!(r.number, 7);
    }

    #[test]
    fn parse_gitlab_url_ssh() {
        let r = parse_origin_url("git@gitlab.com:acme/app.git", Some(Host::GitLab)).unwrap();
        assert_eq!(r.host, Host::GitLab);
        assert_eq!(r.owner, "acme");
        assert_eq!(r.repo, "app");
    }

    #[test]
    fn custom_gitlab_domain() {
        let r = resolve_remote_ref(
            "https://gitlab.company.corp/acme/app/-/merge_requests/99",
            Some(Host::GitLab),
            None,
        )
        .unwrap();
        assert_eq!(r.host, Host::GitLab);
        assert_eq!(r.number, 99);
    }

    #[test]
    fn unsupported_host_fails() {
        let err =
            resolve_remote_ref("https://bitbucket.org/acme/app/pull/1", None, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("bitbucket.org"), "{msg}");
        assert!(
            msg.contains("github.com") || msg.contains("gitlab.com"),
            "{msg}"
        );
    }

    #[test]
    fn bare_id_requires_origin_remote() {
        // No current repo provided -> error.
        let err = resolve_remote_ref("42", Some(Host::GitHub), None).unwrap_err();
        assert!(err.to_string().contains("origin"), "{err}");
    }
}
