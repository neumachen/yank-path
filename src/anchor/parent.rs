//! Parent anchor renderer.
//!
//! Renders paths with `<parent-dir-name>/<cwd-dir-name>` as the leading
//! segments. Targets outside cwd, or a cwd whose parent has no meaningful
//! basename, fall back to the absolute form.

use std::path::{Path, PathBuf};

use crate::anchor::{AnchorRenderer, RenderContext};
use crate::error::YankError;

/// Unit-struct renderer for [`crate::anchor::Anchor::Parent`].
pub struct ParentRenderer;

impl AnchorRenderer for ParentRenderer {
    fn render(&self, target: &Path, ctx: &RenderContext<'_>) -> Result<String, YankError> {
        let cwd_norm = super::normalize_components(&ctx.cwd);

        let cwd_name = match cwd_norm.file_name() {
            Some(n) => n.to_os_string(),
            None => return Ok(super::render_absolute(target, ctx)),
        };
        let parent_name = match cwd_norm.parent().and_then(|p| p.file_name()) {
            Some(n) => n.to_os_string(),
            None => return Ok(super::render_absolute(target, ctx)),
        };

        let mut prefix = PathBuf::from(&parent_name);
        prefix.push(&cwd_name);

        let absolute = super::absolutize(target, &ctx.cwd);
        let normalized = super::normalize_components(&absolute);

        if normalized == cwd_norm {
            return Ok(prefix.display().to_string());
        }

        match normalized.strip_prefix(&cwd_norm) {
            Ok(rel) => {
                let mut out = prefix;
                out.push(rel);
                Ok(out.display().to_string())
            }
            Err(_) => Ok(super::render_absolute(target, ctx)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::test_support::MockFileSystem;

    struct Case {
        name: &'static str,
        cwd: &'static str,
        target: &'static str,
        expected: &'static str,
    }

    #[test]
    fn parent_render_table() {
        let cases = [
            Case {
                name: "dot target resolves to parent/cwd basenames",
                cwd: "/home/u/projects/example-repo",
                target: ".",
                expected: "projects/example-repo",
            },
            Case {
                name: "file in cwd",
                cwd: "/home/u/projects/example-repo",
                target: "README.md",
                expected: "projects/example-repo/README.md",
            },
            Case {
                name: "nested file in cwd",
                cwd: "/home/u/projects/example-repo",
                target: "src/lib.rs",
                expected: "projects/example-repo/src/lib.rs",
            },
            Case {
                name: "target outside cwd falls back to absolute",
                cwd: "/home/u/projects/example-repo",
                target: "/etc/hosts",
                expected: "/etc/hosts",
            },
            Case {
                name: "cwd has no grandparent name falls back to absolute",
                cwd: "/foo",
                target: "bar.txt",
                expected: "/foo/bar.txt",
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
            let got = ParentRenderer.render(Path::new(c.target), &ctx).unwrap();
            assert_eq!(got, c.expected, "case `{}`", c.name);
        }
    }
}
