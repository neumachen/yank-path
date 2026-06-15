//! Absolute anchor renderer.
//!
//! Renders a target as its canonical absolute path. Delegates entirely to
//! the shared `render_absolute` helper in [`super`], which prefers
//! `FileSystem::canonicalize` for existing paths and falls back to a
//! lexical `absolutize` + `normalize_components` for non-existing ones.

use std::path::Path;

use crate::anchor::{AnchorRenderer, RenderContext};
use crate::error::YankError;

/// Unit-struct renderer for [`crate::anchor::Anchor::Absolute`].
pub struct AbsoluteRenderer;

impl AnchorRenderer for AbsoluteRenderer {
    fn render(&self, target: &Path, ctx: &RenderContext<'_>) -> Result<String, YankError> {
        Ok(super::render_absolute(target, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::test_support::MockFileSystem;
    use std::path::PathBuf;

    struct Case {
        name: &'static str,
        cwd: &'static str,
        target: &'static str,
        expected: &'static str,
    }

    #[test]
    fn absolute_render_table() {
        let cases = [
            Case {
                name: "relative target joined with cwd (lexical fallback)",
                cwd: "/home/u/proj",
                target: "README.md",
                expected: "/home/u/proj/README.md",
            },
            Case {
                name: "absolute target passes through",
                cwd: "/home/u/proj",
                target: "/etc/hosts",
                expected: "/etc/hosts",
            },
            Case {
                name: "non-existent nested target uses lexical normalisation",
                cwd: "/home/u/proj",
                target: "src/./lib.rs",
                expected: "/home/u/proj/src/lib.rs",
            },
            Case {
                name: "dot target resolves to cwd",
                cwd: "/home/u/proj",
                target: ".",
                expected: "/home/u/proj",
            },
            Case {
                name: "dotdot target climbs cwd",
                cwd: "/home/u/proj",
                target: "../other",
                expected: "/home/u/other",
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
            let got = AbsoluteRenderer
                .render(Path::new(c.target), &ctx)
                .unwrap();
            assert_eq!(got, c.expected, "case `{}`", c.name);
        }
    }
}
