//! Relative-to anchor renderer.
//!
//! Renders a target as a path relative to a user-supplied base directory.
//! Walks up with `..` when the target is a sibling of the base (i.e. not
//! a descendant). Both base and target are absolutised against
//! `ctx.cwd` and lexically normalised before comparison; the disk is
//! not consulted.

use std::path::{Component, Path, PathBuf};

use crate::anchor::{AnchorRenderer, RenderContext};
use crate::error::YankError;

/// Stateful renderer for [`crate::anchor::Anchor::RelativeTo`].
///
/// Holds the user-supplied base path; constructed once per
/// `--relative-to` invocation by the dispatcher.
pub struct RelativeToRenderer {
    base: PathBuf,
}

impl RelativeToRenderer {
    /// Build a renderer rooted at `base`. `base` may be relative — it
    /// will be absolutised against `ctx.cwd` at render time.
    pub fn new(base: PathBuf) -> Self {
        Self { base }
    }
}

impl AnchorRenderer for RelativeToRenderer {
    fn render(&self, target: &Path, ctx: &RenderContext<'_>) -> Result<String, YankError> {
        let abs = super::normalize_components(&super::absolutize(target, &ctx.cwd));
        let base_abs = super::normalize_components(&super::absolutize(&self.base, &ctx.cwd));

        if abs == base_abs {
            return Ok(".".to_string());
        }

        if let Ok(rel) = abs.strip_prefix(&base_abs) {
            return Ok(PathBuf::from(rel).display().to_string());
        }

        // Compute common-prefix length of components.
        let abs_components: Vec<Component> = abs.components().collect();
        let base_components: Vec<Component> = base_abs.components().collect();
        let mut common = 0usize;
        let min_len = abs_components.len().min(base_components.len());
        while common < min_len && abs_components[common] == base_components[common] {
            common += 1;
        }

        let up = base_components.len() - common;
        let mut out = PathBuf::new();
        for _ in 0..up {
            out.push("..");
        }
        for c in abs_components.iter().skip(common) {
            out.push(c.as_os_str());
        }

        if out.as_os_str().is_empty() {
            Ok(".".to_string())
        } else {
            Ok(out.display().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::test_support::MockFileSystem;

    struct Case {
        name: &'static str,
        base: &'static str,
        cwd: &'static str,
        target: &'static str,
        expected: &'static str,
    }

    #[test]
    fn relative_to_render_table() {
        let cases = [
            Case {
                name: "child via relative target",
                base: "/home/u/projects",
                cwd: "/home/u/projects",
                target: "example-repo",
                expected: "example-repo",
            },
            Case {
                name: "child via cwd-relative dot",
                base: "/home/u/projects",
                cwd: "/home/u/projects/example-repo",
                target: ".",
                expected: "example-repo",
            },
            Case {
                name: "target equals base",
                base: "/home/u/projects/example-repo",
                cwd: "/home/u/projects/example-repo",
                target: ".",
                expected: ".",
            },
            Case {
                name: "sibling via dotdot walk-up",
                base: "/a/b/c",
                cwd: "/a/b/c",
                target: "/a/d/e",
                expected: "../../d/e",
            },
            Case {
                name: "fully disjoint paths walk to common root",
                base: "/a/b",
                cwd: "/a/b",
                target: "/x/y",
                expected: "../../x/y",
            },
        ];

        for c in cases {
            let fs = MockFileSystem::new(c.cwd);
            let ctx = RenderContext {
                cwd: PathBuf::from(c.cwd),
                home: None,
                git_root: None,
                fs: &fs,
            };
            let renderer = RelativeToRenderer::new(PathBuf::from(c.base));
            let got = renderer.render(Path::new(c.target), &ctx).unwrap();
            assert_eq!(got, c.expected, "case `{}`", c.name);
        }
    }
}
