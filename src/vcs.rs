//! VCS metadata provider via direct `.git/` parsing.
//!
//! No `git` subprocess is invoked — we parse `.git/config`, `.git/HEAD`, and
//! ref files directly. This keeps the binary dependency-free of the `git`
//! executable and keeps tests fast (no subprocess overhead).

use std::collections::HashMap;
use std::path::Path;

use crate::error::YankError;

/// VCS metadata resolved from a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcsInfo {
    /// Raw URL from `.git/config` for the chosen remote.
    pub remote_url: String,
    /// 40-hex commit SHA of HEAD, if resolvable.
    pub sha: Option<String>,
    /// Current branch name, if HEAD is a symbolic ref.
    pub branch: Option<String>,
    /// True if `.git/HEAD` is a raw SHA (detached HEAD).
    pub detached: bool,
    /// True if `[branch "<b>"]` in `.git/config` has both `remote` and `merge`.
    pub has_upstream: bool,
    /// True if local ref differs from remote-tracking ref (best-effort; false if unknown).
    pub ahead_of_remote: bool,
}

/// DI trait: resolve VCS metadata for a repository.
pub trait VcsInfoProvider {
    /// Read VCS metadata for the repo rooted at `git_root`, using remote `remote` (e.g. "origin").
    fn info(&self, git_root: &Path, remote: &str) -> Result<VcsInfo, YankError>;
}

/// Real implementation that parses `.git/` files directly.
#[derive(Debug, Default, Clone, Copy)]
pub struct GitDirVcsInfoProvider;

impl GitDirVcsInfoProvider {
    pub fn new() -> Self {
        Self
    }
}

impl VcsInfoProvider for GitDirVcsInfoProvider {
    fn info(&self, git_root: &Path, remote: &str) -> Result<VcsInfo, YankError> {
        let git_dir = git_root.join(".git");

        // Parse .git/config for remote URL and branch tracking info
        let config_path = git_dir.join("config");
        let config_content = std::fs::read_to_string(&config_path)
            .map_err(|e| YankError::Vcs(format!("cannot read .git/config: {e}")))?;

        let remote_url = parse_remote_url(&config_content, remote)?;

        // Parse .git/HEAD for branch/detached state
        let head_path = git_dir.join("HEAD");
        let head_content = std::fs::read_to_string(&head_path)
            .map_err(|e| YankError::Vcs(format!("cannot read .git/HEAD: {e}")))?;
        let head_content = head_content.trim();

        let (branch, detached, sha) = if let Some(ref_path) = head_content.strip_prefix("ref: ") {
            // Symbolic ref: ref: refs/heads/<branch>
            let branch_name = ref_path.strip_prefix("refs/heads/").map(|s| s.to_string());
            let sha = resolve_ref(&git_dir, ref_path);
            (branch_name, false, sha)
        } else if head_content.len() == 40 && head_content.chars().all(|c| c.is_ascii_hexdigit()) {
            // Detached HEAD with raw SHA
            (None, true, Some(head_content.to_string()))
        } else {
            (None, true, None)
        };

        // Check if branch has upstream configured
        let has_upstream = branch
            .as_ref()
            .map(|b| check_has_upstream(&config_content, b))
            .unwrap_or(false);

        // Check if local differs from remote-tracking ref (best-effort)
        let ahead_of_remote = branch
            .as_ref()
            .is_some_and(|b| check_ahead_of_remote(&git_dir, b, remote, sha.as_deref()));

        Ok(VcsInfo {
            remote_url,
            sha,
            branch,
            detached,
            has_upstream,
            ahead_of_remote,
        })
    }
}

/// Parse remote URL from git config content.
fn parse_remote_url(config: &str, remote: &str) -> Result<String, YankError> {
    // Very simple INI-style parser for git config
    let sections = parse_git_config(config);

    let section_name = format!("remote \"{remote}\"");
    let remote_section = sections
        .get(&section_name)
        .ok_or_else(|| YankError::Vcs(format!("remote '{remote}' not found")))?;

    remote_section
        .get("url")
        .cloned()
        .ok_or_else(|| YankError::Vcs(format!("remote '{remote}' has no url")))
}

/// Check if a branch has upstream tracking configured.
fn check_has_upstream(config: &str, branch: &str) -> bool {
    let sections = parse_git_config(config);
    let section_name = format!("branch \"{branch}\"");

    if let Some(branch_section) = sections.get(&section_name) {
        branch_section.contains_key("remote") && branch_section.contains_key("merge")
    } else {
        false
    }
}

/// Simple git config parser. Returns sections as nested HashMaps.
fn parse_git_config(content: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            // Section header: [remote "origin"] or [core]
            current_section = line[1..line.len() - 1].to_string();
            sections.entry(current_section.clone()).or_default();
        } else if let Some((key, value)) = line.split_once('=') {
            // Key-value pair
            if !current_section.is_empty() {
                sections
                    .entry(current_section.clone())
                    .or_default()
                    .insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }

    sections
}

/// Resolve a ref path (e.g. "refs/heads/main") to its SHA.
fn resolve_ref(git_dir: &Path, ref_path: &str) -> Option<String> {
    // First try the loose ref file
    let ref_file = git_dir.join(ref_path);
    if let Ok(content) = std::fs::read_to_string(&ref_file) {
        let sha = content.trim();
        if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(sha.to_string());
        }
    }

    // Fall back to packed-refs
    let packed_refs_path = git_dir.join("packed-refs");
    if let Ok(packed) = std::fs::read_to_string(&packed_refs_path) {
        for line in packed.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            // Format: <sha> <ref>
            if let Some((sha, ref_name)) = line.split_once(' ') {
                if ref_name == ref_path && sha.len() == 40 {
                    return Some(sha.to_string());
                }
            }
        }
    }

    None
}

/// Check if local branch is ahead of remote-tracking branch.
fn check_ahead_of_remote(
    git_dir: &Path,
    branch: &str,
    remote: &str,
    local_sha: Option<&str>,
) -> bool {
    let local_sha = match local_sha {
        Some(s) => s,
        None => return false,
    };

    let remote_ref = format!("refs/remotes/{remote}/{branch}");
    let remote_sha = resolve_ref(git_dir, &remote_ref);

    match remote_sha {
        Some(rs) => local_sha != rs,
        None => false, // Unknown remote ref → not ahead (conservative)
    }
}

// ---------------------------------------------------------------------------
// Test-only fake
// ---------------------------------------------------------------------------

/// Test fake that returns configurable VcsInfo.
#[cfg(test)]
pub struct FakeVcsInfoProvider {
    pub info: VcsInfo,
}

#[cfg(test)]
impl FakeVcsInfoProvider {
    pub fn new(info: VcsInfo) -> Self {
        Self { info }
    }

    pub fn default_github() -> Self {
        Self {
            info: VcsInfo {
                remote_url: "git@github.com:user/repo.git".to_string(),
                sha: Some("abc1234567890123456789012345678901234567".to_string()),
                branch: Some("main".to_string()),
                detached: false,
                has_upstream: true,
                ahead_of_remote: false,
            },
        }
    }
}

#[cfg(test)]
impl VcsInfoProvider for FakeVcsInfoProvider {
    fn info(&self, _git_root: &Path, _remote: &str) -> Result<VcsInfo, YankError> {
        Ok(self.info.clone())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_git_repo(config: &str, head: &str, refs: &[(&str, &str)]) -> tempfile::TempDir {
        let tmp = tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        std::fs::write(git_dir.join("config"), config).unwrap();
        std::fs::write(git_dir.join("HEAD"), head).unwrap();

        for (ref_path, sha) in refs {
            let full_path = git_dir.join(ref_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full_path, sha).unwrap();
        }

        tmp
    }

    #[test]
    fn parses_ssh_remote_url() {
        let config = r#"
[core]
    repositoryformatversion = 0
[remote "origin"]
    url = git@github.com:user/repo.git
    fetch = +refs/heads/*:refs/remotes/origin/*
"#;
        let tmp = setup_git_repo(
            config,
            "ref: refs/heads/main\n",
            &[(
                "refs/heads/main",
                "abc1234567890123456789012345678901234567\n",
            )],
        );

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert_eq!(info.remote_url, "git@github.com:user/repo.git");
    }

    #[test]
    fn parses_https_remote_url() {
        let config = r#"
[remote "origin"]
    url = https://github.com/user/repo.git
"#;
        let tmp = setup_git_repo(
            config,
            "ref: refs/heads/main\n",
            &[(
                "refs/heads/main",
                "abc1234567890123456789012345678901234567\n",
            )],
        );

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert_eq!(info.remote_url, "https://github.com/user/repo.git");
    }

    #[test]
    fn detects_missing_remote() {
        let config = r#"
[remote "origin"]
    url = git@github.com:user/repo.git
"#;
        let tmp = setup_git_repo(config, "ref: refs/heads/main\n", &[]);

        let provider = GitDirVcsInfoProvider::new();
        let err = provider.info(tmp.path(), "upstream").unwrap_err();

        assert!(matches!(err, YankError::Vcs(msg) if msg.contains("upstream")));
    }

    #[test]
    fn resolves_branch_from_symbolic_ref() {
        let config = r#"
[remote "origin"]
    url = git@github.com:user/repo.git
"#;
        let tmp = setup_git_repo(
            config,
            "ref: refs/heads/feature-branch\n",
            &[(
                "refs/heads/feature-branch",
                "def4567890123456789012345678901234567890\n",
            )],
        );

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert_eq!(info.branch.as_deref(), Some("feature-branch"));
        assert!(!info.detached);
        assert_eq!(
            info.sha.as_deref(),
            Some("def4567890123456789012345678901234567890")
        );
    }

    #[test]
    fn detects_detached_head() {
        let config = r#"
[remote "origin"]
    url = git@github.com:user/repo.git
"#;
        let tmp = setup_git_repo(config, "abc1234567890123456789012345678901234567\n", &[]);

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert!(info.detached);
        assert!(info.branch.is_none());
        assert_eq!(
            info.sha.as_deref(),
            Some("abc1234567890123456789012345678901234567")
        );
    }

    #[test]
    fn detects_has_upstream() {
        let config = r#"
[remote "origin"]
    url = git@github.com:user/repo.git
[branch "main"]
    remote = origin
    merge = refs/heads/main
"#;
        let tmp = setup_git_repo(
            config,
            "ref: refs/heads/main\n",
            &[(
                "refs/heads/main",
                "abc1234567890123456789012345678901234567\n",
            )],
        );

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert!(info.has_upstream);
    }

    #[test]
    fn detects_no_upstream_when_branch_config_missing() {
        let config = r#"
[remote "origin"]
    url = git@github.com:user/repo.git
"#;
        let tmp = setup_git_repo(
            config,
            "ref: refs/heads/main\n",
            &[(
                "refs/heads/main",
                "abc1234567890123456789012345678901234567\n",
            )],
        );

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert!(!info.has_upstream);
    }

    #[test]
    fn detects_ahead_of_remote() {
        let config = r#"
[remote "origin"]
    url = git@github.com:user/repo.git
[branch "main"]
    remote = origin
    merge = refs/heads/main
"#;
        let tmp = setup_git_repo(
            config,
            "ref: refs/heads/main\n",
            &[
                (
                    "refs/heads/main",
                    "abc1234567890123456789012345678901234567\n",
                ),
                (
                    "refs/remotes/origin/main",
                    "000000000000000000000000000000000000dead\n",
                ),
            ],
        );

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert!(info.ahead_of_remote);
    }

    #[test]
    fn not_ahead_when_shas_match() {
        let config = r#"
[remote "origin"]
    url = git@github.com:user/repo.git
[branch "main"]
    remote = origin
    merge = refs/heads/main
"#;
        let tmp = setup_git_repo(
            config,
            "ref: refs/heads/main\n",
            &[
                (
                    "refs/heads/main",
                    "abc1234567890123456789012345678901234567\n",
                ),
                (
                    "refs/remotes/origin/main",
                    "abc1234567890123456789012345678901234567\n",
                ),
            ],
        );

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert!(!info.ahead_of_remote);
    }

    #[test]
    fn resolves_sha_from_packed_refs() {
        let config = r#"
[remote "origin"]
    url = git@github.com:user/repo.git
"#;
        let tmp = tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        std::fs::write(git_dir.join("config"), config).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            git_dir.join("packed-refs"),
            "# pack-refs with: peeled fully-peeled sorted\n\
             abc1234567890123456789012345678901234567 refs/heads/main\n",
        )
        .unwrap();

        let provider = GitDirVcsInfoProvider::new();
        let info = provider.info(tmp.path(), "origin").unwrap();

        assert_eq!(
            info.sha.as_deref(),
            Some("abc1234567890123456789012345678901234567")
        );
    }
}
