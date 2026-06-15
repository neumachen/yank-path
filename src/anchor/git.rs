//! Git anchor renderer.
//!
//! Renders paths relative to the enclosing Git repository root. Requires
//! `ctx.git_root` to be set; otherwise returns [`YankError::NotInRepo`].
//! A target outside the repository is also a `NotInRepo` error, since
//! `--from git` implies the user wants a repo-relative answer.

use std::path::{Path, PathBuf};

use crate::anchor::{AnchorRenderer, RenderContext};
use crate::error::YankError;

/// Unit-struct renderer for [`crate::anchor::Anchor::Git`].
pub struct GitRenderer;

impl AnchorRenderer for GitRenderer {
    fn render(&self, target: &Path, ctx: &RenderContext<'_>) -> Result<String, YankError> {
        let git_root = match ctx.git_root.as_ref() {
            Some(r) => r,
            None => return Err(YankError::NotInRepo),
        };

        let absolute = super::absolutize(target, &ctx.cwd);
        let normalized = super::normalize_components(&absolute);
        let root_norm = super::normalize_components(git_root);

        if normalized == root_norm {
            return Ok(".".to_string());
        }

        match normalized.strip_prefix(&root_norm) {
            Ok(rel) => Ok(PathBuf::from(rel).display().to_string()),
            Err(_) => Err(YankError::NotInRepo),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::test_support::MockFileSystem;

    #[test]
    fn dot_at_git_root_yields_dot() {
        let fs = MockFileSystem::new("/home/u/proj");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/proj"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/proj")),
            fs: &fs,
        };
        let got = GitRenderer.render(Path::new("."), &ctx).unwrap();
        assert_eq!(got, ".");
    }

    #[test]
    fn file_at_root_yields_relative() {
        let fs = MockFileSystem::new("/home/u/proj");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/proj"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/proj")),
            fs: &fs,
        };
        let got = GitRenderer.render(Path::new("src/lib.rs"), &ctx).unwrap();
        assert_eq!(got, "src/lib.rs");
    }

    #[test]
    fn file_under_subdir_resolves_via_cwd() {
        let fs = MockFileSystem::new("/home/u/proj/src");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/proj/src"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/proj")),
            fs: &fs,
        };
        let got = GitRenderer.render(Path::new("lib.rs"), &ctx).unwrap();
        assert_eq!(got, "src/lib.rs");
    }

    #[test]
    fn no_git_root_errors() {
        let fs = MockFileSystem::new("/home/u/proj");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/proj"),
            home: None,
            git_root: None,
            fs: &fs,
        };
        let err = GitRenderer.render(Path::new("src/lib.rs"), &ctx).unwrap_err();
        assert!(matches!(err, YankError::NotInRepo));
    }

    #[test]
    fn target_outside_repo_errors() {
        let fs = MockFileSystem::new("/home/u/proj");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/proj"),
            home: None,
            git_root: Some(PathBuf::from("/home/u/proj")),
            fs: &fs,
        };
        let err = GitRenderer
            .render(Path::new("/etc/hosts"), &ctx)
            .unwrap_err();
        assert!(matches!(err, YankError::NotInRepo));
    }
}
