//! Base anchor renderer.
//!
//! Renders paths with the *basename of cwd* as the leading segment, so a
//! file at `/home/u/proj/src/lib.rs` (cwd = `/home/u/proj`) becomes
//! `proj/src/lib.rs`. Targets outside cwd, or a cwd with no meaningful
//! basename (e.g. `/`), fall back to the absolute form.

use std::path::{Path, PathBuf};

use crate::anchor::{AnchorRenderer, RenderContext};
use crate::error::YankError;

/// Unit-struct renderer for [`crate::anchor::Anchor::Base`].
pub struct BaseRenderer;

impl AnchorRenderer for BaseRenderer {
    fn render(&self, target: &Path, ctx: &RenderContext<'_>) -> Result<String, YankError> {
        let absolute = super::absolutize(target, &ctx.cwd);
        let normalized = super::normalize_components(&absolute);
        let cwd_norm = super::normalize_components(&ctx.cwd);

        let basename = match cwd_norm.file_name() {
            Some(n) => n,
            None => return Ok(super::render_absolute(target, ctx)),
        };

        if normalized == cwd_norm {
            return Ok(PathBuf::from(basename).display().to_string());
        }

        match normalized.strip_prefix(&cwd_norm) {
            Ok(rel) => {
                let mut out = PathBuf::from(basename);
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
    fn base_render_table() {
        let cases = [
            Case {
                name: "dot target resolves to cwd basename",
                cwd: "/home/u/proj",
                target: ".",
                expected: "proj",
            },
            Case {
                name: "file in cwd",
                cwd: "/home/u/proj",
                target: "README.md",
                expected: "proj/README.md",
            },
            Case {
                name: "nested file in cwd",
                cwd: "/home/u/proj",
                target: "src/lib.rs",
                expected: "proj/src/lib.rs",
            },
            Case {
                name: "target outside cwd falls back to absolute",
                cwd: "/home/u/proj",
                target: "/etc/hosts",
                expected: "/etc/hosts",
            },
            Case {
                name: "cwd is root falls back to absolute",
                cwd: "/",
                target: "etc/hosts",
                expected: "/etc/hosts",
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
            let got = BaseRenderer.render(Path::new(c.target), &ctx).unwrap();
            assert_eq!(got, c.expected, "case `{}`", c.name);
        }
    }
}
