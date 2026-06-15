//! Home anchor renderer.
//!
//! Renders paths with `$HOME` collapsed to `~`. Targets equal to `$HOME`
//! become `~`, targets inside `$HOME` become `~/<rel>`, and targets
//! outside (or `home == None`) fall back to the absolute form.

use std::path::Path;

use crate::anchor::{AnchorRenderer, RenderContext};
use crate::error::YankError;

/// Unit-struct renderer for [`crate::anchor::Anchor::Home`].
pub struct HomeRenderer;

impl AnchorRenderer for HomeRenderer {
    fn render(&self, target: &Path, ctx: &RenderContext<'_>) -> Result<String, YankError> {
        let abs_string = super::render_absolute(target, ctx);

        let home = match ctx.home.as_ref() {
            Some(h) => h,
            None => return Ok(abs_string),
        };
        let h_str = home.display().to_string();

        if abs_string == h_str {
            return Ok("~".to_string());
        }

        let sep = std::path::MAIN_SEPARATOR;
        let with_slash = format!("{h_str}/");
        let with_native = format!("{h_str}{sep}");

        if let Some(rest) = abs_string.strip_prefix(&with_slash) {
            return Ok(format!("~/{rest}"));
        }
        if sep != '/' {
            if let Some(rest) = abs_string.strip_prefix(&with_native) {
                return Ok(format!("~/{rest}"));
            }
        }

        Ok(abs_string)
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
        home: Option<&'static str>,
        target: &'static str,
        expected: &'static str,
    }

    #[test]
    fn home_render_table() {
        let cases = [
            Case {
                name: "dot under home collapses",
                cwd: "/home/u/proj",
                home: Some("/home/u"),
                target: ".",
                expected: "~/proj",
            },
            Case {
                name: "nested file under home",
                cwd: "/home/u/proj",
                home: Some("/home/u"),
                target: "README.md",
                expected: "~/proj/README.md",
            },
            Case {
                name: "cwd exactly equals home",
                cwd: "/home/u",
                home: Some("/home/u"),
                target: ".",
                expected: "~",
            },
            Case {
                name: "target outside home keeps absolute form",
                cwd: "/home/u/proj",
                home: Some("/home/u"),
                target: "/etc/hosts",
                expected: "/etc/hosts",
            },
            Case {
                name: "no home available — absolute fallback",
                cwd: "/home/u/proj",
                home: None,
                target: ".",
                expected: "/home/u/proj",
            },
        ];

        for c in cases {
            let mut fs = MockFileSystem::new(c.cwd);
            if let Some(h) = c.home {
                fs = fs.with_home(h);
            }
            let ctx = RenderContext {
                cwd: PathBuf::from(c.cwd),
                home: c.home.map(PathBuf::from),
                git_root: None,
                fs: &fs,
            };
            let got = HomeRenderer.render(Path::new(c.target), &ctx).unwrap();
            assert_eq!(got, c.expected, "case `{}`", c.name);
        }
    }
}
