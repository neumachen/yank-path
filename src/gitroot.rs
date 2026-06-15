//! Git repository root detection via filesystem walk-up.
//!
//! No `git` subprocess is invoked — we simply look for a `.git` directory
//! (or file, for worktrees / submodules) in the cwd and each ancestor.

use std::path::{Path, PathBuf};

/// DI trait: find the Git repository root containing `start`, if any.
pub trait GitRootDetector {
    /// Walk up from `start` looking for a `.git` entry. Returns the directory
    /// that contains `.git`, or `None` if no ancestor qualifies.
    fn find_root(&self, start: &Path) -> Option<PathBuf>;
}

/// Real `.git`-walk-up detector.
#[derive(Debug, Default, Clone, Copy)]
pub struct WalkUpGitRootDetector;

impl WalkUpGitRootDetector {
    pub fn new() -> Self {
        Self
    }
}

impl GitRootDetector for WalkUpGitRootDetector {
    fn find_root(&self, start: &Path) -> Option<PathBuf> {
        let mut current: Option<&Path> = Some(start);
        while let Some(dir) = current {
            let candidate = dir.join(".git");
            if candidate.exists() {
                return Some(dir.to_path_buf());
            }
            current = dir.parent();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn finds_root_in_current_dir() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        let det = WalkUpGitRootDetector::new();
        assert_eq!(det.find_root(tmp.path()), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn finds_root_in_ancestor() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        let nested = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).unwrap();
        let det = WalkUpGitRootDetector::new();
        assert_eq!(det.find_root(&nested), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn returns_none_when_no_git() {
        let tmp = tempdir().unwrap();
        let det = WalkUpGitRootDetector::new();
        assert_eq!(det.find_root(tmp.path()), None);
    }

    #[test]
    fn finds_root_when_dot_git_is_file() {
        // Worktrees / submodules use a `.git` file pointing at the real dir.
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join(".git"), b"gitdir: somewhere").unwrap();
        let det = WalkUpGitRootDetector::new();
        assert_eq!(det.find_root(tmp.path()), Some(tmp.path().to_path_buf()));
    }
}
