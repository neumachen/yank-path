//! Anchor engine.
//!
//! An [`Anchor`] selects how a target path is *rendered* as a string —
//! `~/...`, `basename/...`, `parent/basename/...`, repo-relative, absolute,
//! or relative to an arbitrary base directory. Each variant is rendered by
//! a distinct unit-struct implementing [`AnchorRenderer`], and the
//! [`render_with`] dispatcher picks the right one for an [`Anchor`] value.
//!
//! This satisfies the Open/Closed Principle: adding a new anchor means
//! adding a variant + a new renderer + a new match arm, never modifying
//! existing renderers.

use std::path::{Component, Path, PathBuf};

use crate::error::YankError;
use crate::fs::FileSystem;

pub mod absolute;
pub mod base;
pub mod git;
pub mod home;
pub mod parent;
pub mod relative_to;

pub use absolute::AbsoluteRenderer;
pub use base::BaseRenderer;
pub use git::GitRenderer;
pub use home::HomeRenderer;
pub use parent::ParentRenderer;
pub use relative_to::RelativeToRenderer;

/// Anchor selection — one variant per `--from` / `--absolute` /
/// `--relative-to` option exposed by the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// `~/...` form when target is inside `$HOME`; fall back to absolute
    /// otherwise.
    Home,
    /// Leading segment is the current directory's *name* (basename).
    Base,
    /// Leading segments are `<parent-dir-name>/<cwd-dir-name>`.
    Parent,
    /// Path relative to the enclosing Git repository root.
    Git,
    /// Canonical absolute path.
    Absolute,
    /// Path relative to a user-supplied base directory.
    RelativeTo(PathBuf),
}

/// Read-only context passed to every renderer.
///
/// Holds the cwd, the optional `$HOME`, the optional Git root, and a
/// `FileSystem` for any disk-touching operations renderers need (such as
/// canonicalising existing paths).
pub struct RenderContext<'a> {
    /// Current working directory (already resolved).
    pub cwd: PathBuf,
    /// User home directory, if known.
    pub home: Option<PathBuf>,
    /// Git repository root, if discoverable.
    pub git_root: Option<PathBuf>,
    /// Filesystem abstraction for disk-touching operations.
    pub fs: &'a dyn FileSystem,
}

/// Strategy trait: render a single target path under one anchor.
pub trait AnchorRenderer {
    /// Render `target` as a `String` under this renderer's anchor.
    fn render(&self, target: &Path, ctx: &RenderContext<'_>) -> Result<String, YankError>;
}

/// Dispatcher: pick the right renderer for an [`Anchor`] value and run it.
pub fn render_with(
    anchor: &Anchor,
    target: &Path,
    ctx: &RenderContext<'_>,
) -> Result<String, YankError> {
    match anchor {
        Anchor::Home => HomeRenderer.render(target, ctx),
        Anchor::Base => BaseRenderer.render(target, ctx),
        Anchor::Parent => ParentRenderer.render(target, ctx),
        Anchor::Git => GitRenderer.render(target, ctx),
        Anchor::Absolute => AbsoluteRenderer.render(target, ctx),
        Anchor::RelativeTo(base) => RelativeToRenderer::new(base.clone()).render(target, ctx),
    }
}

// ---------------------------------------------------------------------------
// Shared path helpers (crate-private).
// ---------------------------------------------------------------------------

/// Make `target` absolute by joining with `cwd` when relative.
///
/// Does **not** touch the disk; in particular, the result is not
/// canonicalised. Use [`normalize_components`] afterwards to collapse `.`
/// and `..` lexically.
pub(crate) fn absolutize(target: &Path, cwd: &Path) -> PathBuf {
    if target.is_absolute() {
        target.to_path_buf()
    } else {
        cwd.join(target)
    }
}

/// Collapse `.` and `..` components *lexically*, without consulting the
/// filesystem.
///
/// Semantics:
/// * `Component::CurDir` (`.`) is dropped.
/// * `Component::ParentDir` (`..`) pops the previous normal component if any;
///   otherwise it is preserved (so `../foo` against a relative path stays
///   `../foo`).
/// * Root and prefix (Windows) components are preserved at the front.
pub(crate) fn normalize_components(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                let pop = matches!(out.last(), Some(Component::Normal(_)));
                if pop {
                    out.pop();
                } else {
                    // No normal component to cancel — keep `..` (relative
                    // case) unless we are sitting directly on a root, in
                    // which case `/..` collapses to `/`.
                    let on_root = matches!(
                        out.last(),
                        Some(Component::RootDir) | Some(Component::Prefix(_))
                    );
                    if !on_root {
                        out.push(comp);
                    }
                }
            }
            _ => out.push(comp),
        }
    }
    let mut buf = PathBuf::new();
    for c in out {
        buf.push(c.as_os_str());
    }
    if buf.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        buf
    }
}

/// Render `target` as an absolute, lexically-normalised string.
///
/// Tries `ctx.fs.canonicalize` first (handles symlinks and `..` correctly
/// when the path exists), and falls back to a lexical resolution against
/// `ctx.cwd` when the target does not exist on disk.
pub(crate) fn render_absolute(target: &Path, ctx: &RenderContext<'_>) -> String {
    if ctx.fs.exists(target) {
        if let Ok(canon) = ctx.fs.canonicalize(target) {
            return canon.display().to_string();
        }
    }
    let joined = absolutize(target, &ctx.cwd);
    normalize_components(&joined).display().to_string()
}

// ---------------------------------------------------------------------------
// Test support — a small in-memory FileSystem mock shared by all renderer
// test modules.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod test_support {
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    use crate::error::YankError;
    use crate::fs::FileSystem;

    /// Minimal in-memory `FileSystem` used by anchor unit tests.
    pub(crate) struct MockFileSystem {
        cwd: PathBuf,
        home: Option<PathBuf>,
        existing: RefCell<HashSet<PathBuf>>,
        dirs: RefCell<HashSet<PathBuf>>,
    }

    impl MockFileSystem {
        pub(crate) fn new(cwd: impl Into<PathBuf>) -> Self {
            Self {
                cwd: cwd.into(),
                home: None,
                existing: RefCell::new(HashSet::new()),
                dirs: RefCell::new(HashSet::new()),
            }
        }

        pub(crate) fn with_home(mut self, home: impl Into<PathBuf>) -> Self {
            self.home = Some(home.into());
            self
        }

        pub(crate) fn with_existing_file(self, path: impl Into<PathBuf>) -> Self {
            self.existing.borrow_mut().insert(path.into());
            self
        }

        pub(crate) fn with_existing_dir(self, path: impl Into<PathBuf>) -> Self {
            let p: PathBuf = path.into();
            self.existing.borrow_mut().insert(p.clone());
            self.dirs.borrow_mut().insert(p);
            self
        }
    }

    impl FileSystem for MockFileSystem {
        fn cwd(&self) -> Result<PathBuf, YankError> {
            Ok(self.cwd.clone())
        }

        fn home(&self) -> Option<PathBuf> {
            self.home.clone()
        }

        fn exists(&self, path: &Path) -> bool {
            self.existing.borrow().contains(path)
        }

        fn canonicalize(&self, path: &Path) -> Result<PathBuf, YankError> {
            // The mock treats every "existing" path as already canonical.
            if self.existing.borrow().contains(path) {
                Ok(path.to_path_buf())
            } else {
                Err(YankError::NotFound(path.to_path_buf()))
            }
        }

        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.borrow().contains(path)
        }

        fn is_file(&self, path: &Path) -> bool {
            self.existing.borrow().contains(path) && !self.dirs.borrow().contains(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::MockFileSystem;

    #[test]
    fn dispatcher_routes_to_absolute() {
        let fs = MockFileSystem::new("/home/u/proj");
        let ctx = RenderContext {
            cwd: PathBuf::from("/home/u/proj"),
            home: None,
            git_root: None,
            fs: &fs,
        };
        let s = render_with(&Anchor::Absolute, Path::new("README.md"), &ctx).unwrap();
        assert_eq!(s, "/home/u/proj/README.md");
    }

    #[test]
    fn normalize_collapses_dot_and_dotdot() {
        assert_eq!(
            normalize_components(Path::new("/a/b/./c/../d")),
            PathBuf::from("/a/b/d")
        );
        assert_eq!(
            normalize_components(Path::new("a/./b/../c")),
            PathBuf::from("a/c")
        );
        // Relative `..` with no normal to cancel is preserved.
        assert_eq!(
            normalize_components(Path::new("../foo")),
            PathBuf::from("../foo")
        );
        // `/..` collapses to `/`.
        assert_eq!(normalize_components(Path::new("/..")), PathBuf::from("/"));
    }

    #[test]
    fn absolutize_passes_through_absolute() {
        assert_eq!(
            absolutize(Path::new("/x/y"), Path::new("/cwd")),
            PathBuf::from("/x/y")
        );
        assert_eq!(
            absolutize(Path::new("y"), Path::new("/cwd")),
            PathBuf::from("/cwd/y")
        );
    }
}
