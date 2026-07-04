//! VCS URL anchor renderer.
//!
//! Renders paths as VCS remote URLs (e.g. GitHub blob permalinks). Requires
//! resolved [`VcsInfo`] and the git root to be known. The renderer maps
//! popular hosts (GitHub, GitLab, Bitbucket) to their specific URL formats;
//! unknown hosts get a generic `blob`-style fallback.
//!
//! This renderer is stateful — it is constructed with the resolved VcsInfo
//! rather than resolving it at render time. This keeps the renderer pure
//! and testable.

use std::path::{Path, PathBuf};

use crate::anchor::{AnchorRenderer, RenderContext};
use crate::error::YankError;
use crate::vcs::VcsInfo;

/// Stateful renderer for [`crate::anchor::Anchor::Vcs`].
///
/// Constructed with pre-resolved VcsInfo and optional branch fallback config.
pub struct VcsRenderer {
    info: VcsInfo,
    default_branch: Option<String>,
    branch_fallback: bool,
}

impl VcsRenderer {
    /// Build a VCS URL renderer.
    ///
    /// # Arguments
    /// * `info` — Resolved VCS metadata from the provider.
    /// * `default_branch` — User override for the default branch (from `--vcs-default-branch`).
    /// * `branch_fallback` — If true, fall back to branch name when SHA is unavailable.
    pub fn new(info: VcsInfo, default_branch: Option<String>, branch_fallback: bool) -> Self {
        Self {
            info,
            default_branch,
            branch_fallback,
        }
    }
}

impl AnchorRenderer for VcsRenderer {
    fn render(&self, target: &Path, ctx: &RenderContext<'_>) -> Result<String, YankError> {
        // Need git_root to compute repo-relative path
        let git_root = ctx
            .git_root
            .as_ref()
            .ok_or_else(|| YankError::Vcs("not in a repository".to_string()))?;

        // Compute repo-relative path (same logic as GitRenderer)
        let absolute = super::absolutize(target, &ctx.cwd);
        let normalized = super::normalize_components(&absolute);
        let root_norm = super::normalize_components(git_root);

        let relative_path = if normalized == root_norm {
            // Target is repo root — no trailing path component
            PathBuf::new()
        } else {
            normalized
                .strip_prefix(&root_norm)
                .map_err(|_| YankError::Vcs("target outside repository".to_string()))?
                .to_path_buf()
        };

        // Parse remote URL to extract host/owner/repo
        let (host, owner, repo) = parse_remote_url(&self.info.remote_url)?;

        // Determine the ref (SHA or branch) to use in the URL
        let git_ref = self.resolve_ref()?;

        // Build the URL based on the host
        let url = build_url(&host, &owner, &repo, &git_ref, &relative_path);

        Ok(url)
    }
}

impl VcsRenderer {
    /// Resolve which ref (SHA or branch) to use in the URL.
    fn resolve_ref(&self) -> Result<String, YankError> {
        // Prefer SHA if available (makes permalinks)
        if let Some(ref sha) = self.info.sha {
            return Ok(sha.clone());
        }

        // Fall back to branch if allowed
        if self.branch_fallback {
            // Use explicit default, or current branch, or "main"
            let branch = self
                .default_branch
                .clone()
                .or_else(|| self.info.branch.clone())
                .unwrap_or_else(|| "main".to_string());
            return Ok(branch);
        }

        Err(YankError::Vcs(
            "could not resolve commit SHA (use --vcs-branch-fallback)".to_string(),
        ))
    }
}

/// Parsed remote URL components.
struct RemoteComponents {
    host: String,
    owner: String,
    repo: String,
}

/// Parse a git remote URL into (host, owner, repo).
///
/// Handles:
/// - SSH: `git@github.com:user/repo.git` or `git@github.com:user/repo`
/// - SSH with scheme: `ssh://git@github.com/user/repo.git`
/// - HTTPS: `https://github.com/user/repo.git` or `https://github.com/user/repo`
fn parse_remote_url(url: &str) -> Result<(String, String, String), YankError> {
    let parsed = parse_remote_url_inner(url)
        .ok_or_else(|| YankError::Vcs(format!("cannot parse remote URL: {url}")))?;
    Ok((parsed.host, parsed.owner, parsed.repo))
}

fn parse_remote_url_inner(url: &str) -> Option<RemoteComponents> {
    // SSH format: git@host:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return parse_path_to_owner_repo(host, path);
        }
    }

    // ssh:// format: ssh://git@host/owner/repo.git
    if let Some(rest) = url.strip_prefix("ssh://") {
        // Strip optional user@ prefix
        let rest = if let Some(idx) = rest.find('@') {
            &rest[idx + 1..]
        } else {
            rest
        };
        // Split host/path
        if let Some((host, path)) = rest.split_once('/') {
            return parse_path_to_owner_repo(host, path);
        }
    }

    // HTTPS format: https://host/owner/repo.git
    if let Some(rest) = url.strip_prefix("https://") {
        if let Some((host, path)) = rest.split_once('/') {
            return parse_path_to_owner_repo(host, path);
        }
    }

    // HTTP format (less common): http://host/owner/repo.git
    if let Some(rest) = url.strip_prefix("http://") {
        if let Some((host, path)) = rest.split_once('/') {
            return parse_path_to_owner_repo(host, path);
        }
    }

    None
}

/// Parse "owner/repo.git" or "owner/repo" into owner and repo.
fn parse_path_to_owner_repo(host: &str, path: &str) -> Option<RemoteComponents> {
    // Strip any port from host (e.g., gitlab.example.com:8443)
    let host = host.split(':').next().unwrap_or(host);

    // Path may be "owner/repo.git" or "owner/repo" or "owner/repo/"
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);

    // Split owner/repo
    let (owner, repo) = path.split_once('/')?;

    // Some URLs have nested paths like "org/group/repo" — we take the last segment as repo
    // and everything before the last slash as owner
    let path_segments: Vec<&str> = path.split('/').collect();
    if path_segments.len() >= 2 {
        let repo = path_segments.last()?;
        let owner = path_segments[..path_segments.len() - 1].join("/");
        Some(RemoteComponents {
            host: host.to_string(),
            owner,
            repo: repo.to_string(),
        })
    } else {
        Some(RemoteComponents {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }
}

/// Build the final URL for the given host.
///
/// URL formats by host:
/// - `github.com`: `https://github.com/{owner}/{repo}/blob/{ref}/{path}`
/// - `gitlab.com`: `https://gitlab.com/{owner}/{repo}/-/blob/{ref}/{path}`
/// - `bitbucket.org`: `https://bitbucket.org/{owner}/{repo}/src/{ref}/{path}`
/// - Other hosts: `https://{host}/{owner}/{repo}/blob/{ref}/{path}` (generic fallback)
///
/// When the target is the repository root (empty path), the URL links to the tree root
/// without a trailing path component.
fn build_url(host: &str, owner: &str, repo: &str, git_ref: &str, relative_path: &Path) -> String {
    let path_str = relative_path.display().to_string();
    let path_suffix = if path_str.is_empty() {
        String::new()
    } else {
        format!("/{path_str}")
    };

    match host {
        "github.com" => {
            format!("https://github.com/{owner}/{repo}/blob/{git_ref}{path_suffix}")
        }
        "gitlab.com" => {
            format!("https://gitlab.com/{owner}/{repo}/-/blob/{git_ref}{path_suffix}")
        }
        "bitbucket.org" => {
            format!("https://bitbucket.org/{owner}/{repo}/src/{git_ref}{path_suffix}")
        }
        _ => {
            // Generic fallback for unknown hosts (self-hosted GitLab, Gitea, etc.)
            format!("https://{host}/{owner}/{repo}/blob/{git_ref}{path_suffix}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::test_support::MockFileSystem;
    use crate::vcs::VcsInfo;

    fn make_info(remote_url: &str, sha: Option<&str>, branch: Option<&str>) -> VcsInfo {
        VcsInfo {
            remote_url: remote_url.to_string(),
            sha: sha.map(|s| s.to_string()),
            branch: branch.map(|s| s.to_string()),
            detached: sha.is_some() && branch.is_none(),
            has_upstream: true,
            ahead_of_remote: false,
        }
    }

    // -----------------------------------------------------------------------
    // URL parsing tests
    // -----------------------------------------------------------------------

    #[test]
    fn parses_ssh_url() {
        let (host, owner, repo) = parse_remote_url("git@github.com:user/repo.git").unwrap();
        assert_eq!(host, "github.com");
        assert_eq!(owner, "user");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parses_ssh_url_without_git_suffix() {
        let (host, owner, repo) = parse_remote_url("git@github.com:user/repo").unwrap();
        assert_eq!(host, "github.com");
        assert_eq!(owner, "user");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parses_https_url() {
        let (host, owner, repo) = parse_remote_url("https://github.com/user/repo.git").unwrap();
        assert_eq!(host, "github.com");
        assert_eq!(owner, "user");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parses_https_url_without_git_suffix() {
        let (host, owner, repo) = parse_remote_url("https://github.com/user/repo").unwrap();
        assert_eq!(host, "github.com");
        assert_eq!(owner, "user");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parses_ssh_scheme_url() {
        let (host, owner, repo) = parse_remote_url("ssh://git@github.com/user/repo.git").unwrap();
        assert_eq!(host, "github.com");
        assert_eq!(owner, "user");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parses_gitlab_nested_group() {
        let (host, owner, repo) =
            parse_remote_url("git@gitlab.com:org/group/subgroup/repo.git").unwrap();
        assert_eq!(host, "gitlab.com");
        assert_eq!(owner, "org/group/subgroup");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn unparseable_url_returns_error() {
        let err = parse_remote_url("not-a-valid-url").unwrap_err();
        assert!(matches!(err, YankError::Vcs(msg) if msg.contains("cannot parse")));
    }

    // -----------------------------------------------------------------------
    // URL building tests
    // -----------------------------------------------------------------------

    #[test]
    fn builds_github_url_with_sha() {
        let url = build_url(
            "github.com",
            "user",
            "repo",
            "abc123",
            Path::new("src/main.rs"),
        );
        assert_eq!(url, "https://github.com/user/repo/blob/abc123/src/main.rs");
    }

    #[test]
    fn builds_github_url_for_repo_root() {
        let url = build_url("github.com", "user", "repo", "abc123", Path::new(""));
        assert_eq!(url, "https://github.com/user/repo/blob/abc123");
    }

    #[test]
    fn builds_gitlab_url() {
        let url = build_url(
            "gitlab.com",
            "user",
            "repo",
            "abc123",
            Path::new("src/lib.rs"),
        );
        assert_eq!(url, "https://gitlab.com/user/repo/-/blob/abc123/src/lib.rs");
    }

    #[test]
    fn builds_bitbucket_url() {
        let url = build_url(
            "bitbucket.org",
            "user",
            "repo",
            "abc123",
            Path::new("README.md"),
        );
        assert_eq!(url, "https://bitbucket.org/user/repo/src/abc123/README.md");
    }

    #[test]
    fn builds_generic_url_for_unknown_host() {
        let url = build_url(
            "git.example.com",
            "user",
            "repo",
            "abc123",
            Path::new("file.txt"),
        );
        assert_eq!(
            url,
            "https://git.example.com/user/repo/blob/abc123/file.txt"
        );
    }

    // -----------------------------------------------------------------------
    // Renderer integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn renders_file_with_sha() {
        let fs = MockFileSystem::new("/home/u/repo");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/repo"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/repo")),
            fs: &fs,
        };

        let info = make_info(
            "git@github.com:user/repo.git",
            Some("abc1234567890123456789012345678901234567"),
            Some("main"),
        );
        let renderer = VcsRenderer::new(info, None, false);
        let url = renderer.render(Path::new("src/lib.rs"), &ctx).unwrap();

        assert_eq!(
            url,
            "https://github.com/user/repo/blob/abc1234567890123456789012345678901234567/src/lib.rs"
        );
    }

    #[test]
    fn renders_repo_root() {
        let fs = MockFileSystem::new("/home/u/repo");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/repo"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/repo")),
            fs: &fs,
        };

        let info = make_info(
            "git@github.com:user/repo.git",
            Some("abc1234567890123456789012345678901234567"),
            Some("main"),
        );
        let renderer = VcsRenderer::new(info, None, false);
        let url = renderer.render(Path::new("."), &ctx).unwrap();

        assert_eq!(
            url,
            "https://github.com/user/repo/blob/abc1234567890123456789012345678901234567"
        );
    }

    #[test]
    fn branch_fallback_when_no_sha() {
        let fs = MockFileSystem::new("/home/u/repo");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/repo"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/repo")),
            fs: &fs,
        };

        let info = make_info("git@github.com:user/repo.git", None, Some("develop"));
        let renderer = VcsRenderer::new(info, None, true); // branch_fallback = true
        let url = renderer.render(Path::new("file.txt"), &ctx).unwrap();

        assert_eq!(url, "https://github.com/user/repo/blob/develop/file.txt");
    }

    #[test]
    fn default_branch_override() {
        let fs = MockFileSystem::new("/home/u/repo");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/repo"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/repo")),
            fs: &fs,
        };

        let info = make_info("git@github.com:user/repo.git", None, Some("develop"));
        let renderer = VcsRenderer::new(info, Some("master".to_string()), true);
        let url = renderer.render(Path::new("file.txt"), &ctx).unwrap();

        // default_branch override takes precedence over current branch
        assert_eq!(url, "https://github.com/user/repo/blob/master/file.txt");
    }

    #[test]
    fn error_when_no_sha_and_no_fallback() {
        let fs = MockFileSystem::new("/home/u/repo");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/repo"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/repo")),
            fs: &fs,
        };

        let info = make_info("git@github.com:user/repo.git", None, Some("main"));
        let renderer = VcsRenderer::new(info, None, false); // branch_fallback = false
        let err = renderer.render(Path::new("file.txt"), &ctx).unwrap_err();

        assert!(matches!(err, YankError::Vcs(msg) if msg.contains("--vcs-branch-fallback")));
    }

    #[test]
    fn error_when_no_git_root() {
        let fs = MockFileSystem::new("/home/u/proj");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/proj"),
            home: None,
            git_root: None, // No git root
            fs: &fs,
        };

        let info = make_info("git@github.com:user/repo.git", Some("abc123"), Some("main"));
        let renderer = VcsRenderer::new(info, None, false);
        let err = renderer.render(Path::new("file.txt"), &ctx).unwrap_err();

        assert!(matches!(err, YankError::Vcs(msg) if msg.contains("not in a repository")));
    }

    #[test]
    fn error_when_target_outside_repo() {
        let fs = MockFileSystem::new("/home/u/repo");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/repo"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/repo")),
            fs: &fs,
        };

        let info = make_info("git@github.com:user/repo.git", Some("abc123"), Some("main"));
        let renderer = VcsRenderer::new(info, None, false);
        let err = renderer.render(Path::new("/etc/passwd"), &ctx).unwrap_err();

        assert!(matches!(err, YankError::Vcs(msg) if msg.contains("outside repository")));
    }

    #[test]
    fn handles_subdirectory_cwd() {
        let fs = MockFileSystem::new("/home/u/repo/src");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/repo/src"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/repo")),
            fs: &fs,
        };

        let info = make_info("git@github.com:user/repo.git", Some("abc123"), Some("main"));
        let renderer = VcsRenderer::new(info, None, false);
        let url = renderer.render(Path::new("lib.rs"), &ctx).unwrap();

        assert_eq!(url, "https://github.com/user/repo/blob/abc123/src/lib.rs");
    }
}
