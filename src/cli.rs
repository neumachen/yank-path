//! Command-line interface (clap derive).
//!
//! Defines the user-facing surface only. All defaulting and semantic
//! validation (anchor resolution, operand defaults, mutual-exclusion
//! belt-and-braces) lives here as small, pure helpers so that
//! [`crate::app::App`] can stay focussed on the pipeline.

use std::path::PathBuf;

use clap::{ArgGroup, Parser, ValueEnum};

use crate::anchor::Anchor;
use crate::error::YankError;

/// Render and yank path strings under a chosen anchor.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "yank-path",
    version,
    about = "Render and yank path strings in a chosen anchor form",
    // `--from`, `--absolute`, `--relative-to`, `--vcs` are mutually exclusive.
    group(
        ArgGroup::new("anchor")
            .args(["from", "absolute", "relative_to", "vcs"])
            .multiple(false)
            .required(false),
    ),
)]
pub struct Cli {
    /// Path operands. When empty (and no `--glob` is given) the App
    /// defaults this to `["."]`.
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Anchor selection: `home`, `base`, `parent`, or `git`. Aliases:
    /// `basename` → `base`, `dirname` → `parent`.
    #[arg(long = "from", value_name = "ANCHOR", value_enum)]
    pub from: Option<FromAnchor>,

    /// Render paths relative to this base directory.
    #[arg(long = "relative-to", value_name = "PATH")]
    pub relative_to: Option<PathBuf>,

    /// Render paths in canonical absolute form.
    #[arg(long = "absolute")]
    pub absolute: bool,

    /// Single-level glob pattern(s). Repeatable. Patterns containing `/`
    /// or `**` are rejected.
    #[arg(long = "glob", value_name = "PATTERN")]
    pub glob: Vec<String>,

    /// Also print rendered paths to stdout (does not imply `--no-copy`).
    #[arg(long = "print")]
    pub print: bool,

    /// Do not touch the system clipboard.
    #[arg(long = "no-copy")]
    pub no_copy: bool,

    /// Render paths as VCS remote URLs (e.g. GitHub permalink).
    #[arg(long = "vcs", visible_alias = "VCS")]
    pub vcs: bool,

    /// Remote name for `--vcs` (defaults to `origin`).
    #[arg(long = "vcs-remote", value_name = "REMOTE", requires = "vcs")]
    pub vcs_remote: Option<String>,

    /// Default branch for `--vcs` (defaults to `main`).
    #[arg(long = "vcs-default-branch", value_name = "BRANCH", requires = "vcs")]
    pub vcs_default_branch: Option<String>,

    /// Fall back to branch name when SHA is unavailable (for `--vcs`).
    #[arg(long = "vcs-branch-fallback", requires = "vcs")]
    pub vcs_branch_fallback: bool,
}

/// `--from` enum, with the documented aliases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FromAnchor {
    /// `~/...` form when target is inside `$HOME`.
    Home,
    /// Anchor at the cwd's basename.
    #[value(alias = "basename")]
    Base,
    /// Anchor at `<parent>/<cwd>`.
    #[value(alias = "dirname")]
    Parent,
    /// Anchor at the enclosing Git repository root.
    Git,
}

impl From<FromAnchor> for Anchor {
    fn from(value: FromAnchor) -> Self {
        match value {
            FromAnchor::Home => Anchor::Home,
            FromAnchor::Base => Anchor::Base,
            FromAnchor::Parent => Anchor::Parent,
            FromAnchor::Git => Anchor::Git,
        }
    }
}

impl Cli {
    /// Resolve the user's anchor choice.
    ///
    /// * Returns [`Anchor::Home`] when no anchor option was supplied.
    /// * Returns [`YankError::ConflictingAnchors`] if more than one of
    ///   `--from`, `--absolute`, `--relative-to`, `--vcs` was given — this is
    ///   belt-and-braces in case clap's `ArgGroup` is bypassed (e.g. by
    ///   constructing a `Cli` value directly in a test).
    pub fn anchor(&self) -> Result<Anchor, YankError> {
        let mut count = 0;
        if self.from.is_some() {
            count += 1;
        }
        if self.absolute {
            count += 1;
        }
        if self.relative_to.is_some() {
            count += 1;
        }
        if self.vcs {
            count += 1;
        }
        if count > 1 {
            return Err(YankError::ConflictingAnchors);
        }

        if let Some(from) = self.from {
            return Ok(from.into());
        }
        if self.absolute {
            return Ok(Anchor::Absolute);
        }
        if let Some(base) = &self.relative_to {
            return Ok(Anchor::RelativeTo(base.clone()));
        }
        if self.vcs {
            return Ok(Anchor::Vcs);
        }
        Ok(Anchor::Home)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Cli {
        // `clap` expects argv[0] to be the program name.
        let mut full = vec!["yank-path"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).expect("parse should succeed")
    }

    #[test]
    fn parses_no_args_defaults_to_home_anchor_and_empty_paths() {
        let cli = parse(&[]);
        assert!(cli.paths.is_empty());
        assert_eq!(cli.anchor().unwrap(), Anchor::Home);
        assert!(!cli.print);
        assert!(!cli.no_copy);
        assert!(cli.glob.is_empty());
    }

    #[test]
    fn parses_from_home() {
        let cli = parse(&["--from", "home"]);
        assert_eq!(cli.anchor().unwrap(), Anchor::Home);
    }

    #[test]
    fn parses_from_base() {
        let cli = parse(&["--from", "base"]);
        assert_eq!(cli.anchor().unwrap(), Anchor::Base);
    }

    #[test]
    fn parses_from_parent() {
        let cli = parse(&["--from", "parent"]);
        assert_eq!(cli.anchor().unwrap(), Anchor::Parent);
    }

    #[test]
    fn parses_from_git() {
        let cli = parse(&["--from", "git"]);
        assert_eq!(cli.anchor().unwrap(), Anchor::Git);
    }

    #[test]
    fn alias_basename_maps_to_base() {
        let cli = parse(&["--from", "basename"]);
        assert_eq!(cli.anchor().unwrap(), Anchor::Base);
    }

    #[test]
    fn alias_dirname_maps_to_parent() {
        let cli = parse(&["--from", "dirname"]);
        assert_eq!(cli.anchor().unwrap(), Anchor::Parent);
    }

    #[test]
    fn parses_absolute_flag() {
        let cli = parse(&["--absolute"]);
        assert_eq!(cli.anchor().unwrap(), Anchor::Absolute);
    }

    #[test]
    fn parses_relative_to() {
        let cli = parse(&["--relative-to", "/tmp/base"]);
        assert_eq!(
            cli.anchor().unwrap(),
            Anchor::RelativeTo(PathBuf::from("/tmp/base"))
        );
    }

    #[test]
    fn parses_repeated_glob_and_positional_paths() {
        let cli = parse(&["--glob", "*.rs", "--glob", "*.md", "a.txt", "b.txt"]);
        assert_eq!(cli.glob, vec!["*.rs".to_string(), "*.md".to_string()]);
        assert_eq!(
            cli.paths,
            vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")]
        );
    }

    #[test]
    fn parses_print_and_no_copy_flags() {
        let cli = parse(&["--print", "--no-copy"]);
        assert!(cli.print);
        assert!(cli.no_copy);
    }

    #[test]
    fn clap_rejects_conflicting_anchor_options() {
        let err = Cli::try_parse_from(["yank-path", "--absolute", "--from", "home"]).unwrap_err();
        // clap returns an `ArgumentConflict` for ArgGroup violations.
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "expected ArgumentConflict, got: {err}"
        );
    }

    #[test]
    fn anchor_helper_detects_conflict_when_bypassed() {
        // Bypass clap by constructing the struct directly: simulate two
        // anchors set at once.
        let cli = Cli {
            paths: vec![],
            from: Some(FromAnchor::Home),
            relative_to: None,
            absolute: true,
            glob: vec![],
            print: false,
            no_copy: false,
            vcs: false,
            vcs_remote: None,
            vcs_default_branch: None,
            vcs_branch_fallback: false,
        };
        match cli.anchor() {
            Err(YankError::ConflictingAnchors) => {}
            other => panic!("expected ConflictingAnchors, got {other:?}"),
        }
    }

    #[test]
    fn anchor_helper_detects_three_way_conflict() {
        let cli = Cli {
            paths: vec![],
            from: Some(FromAnchor::Git),
            relative_to: Some(PathBuf::from("/x")),
            absolute: true,
            glob: vec![],
            print: false,
            no_copy: false,
            vcs: false,
            vcs_remote: None,
            vcs_default_branch: None,
            vcs_branch_fallback: false,
        };
        match cli.anchor() {
            Err(YankError::ConflictingAnchors) => {}
            other => panic!("expected ConflictingAnchors, got {other:?}"),
        }
    }

    // --- VCS flag tests (Phase 1) ---

    #[test]
    fn parses_vcs_flag_and_returns_vcs_anchor() {
        let cli = parse(&["--vcs"]);
        assert!(cli.vcs);
        assert_eq!(cli.anchor().unwrap(), Anchor::Vcs);
    }

    #[test]
    fn parses_uppercase_vcs_alias() {
        let cli = parse(&["--VCS"]);
        assert!(cli.vcs);
        assert_eq!(cli.anchor().unwrap(), Anchor::Vcs);
    }

    #[test]
    fn vcs_conflicts_with_absolute() {
        let err = Cli::try_parse_from(["yank-path", "--vcs", "--absolute"]).unwrap_err();
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "expected ArgumentConflict, got: {err}"
        );
    }

    #[test]
    fn vcs_remote_requires_vcs() {
        let err = Cli::try_parse_from(["yank-path", "--vcs-remote", "origin"]).unwrap_err();
        // Clap should reject missing required `--vcs`.
        assert!(
            matches!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument),
            "expected MissingRequiredArgument, got: {err}"
        );
    }

    #[test]
    fn vcs_default_branch_requires_vcs() {
        let err = Cli::try_parse_from(["yank-path", "--vcs-default-branch", "main"]).unwrap_err();
        assert!(
            matches!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument),
            "expected MissingRequiredArgument, got: {err}"
        );
    }

    #[test]
    fn parses_vcs_with_remote_and_default_branch() {
        let cli = parse(&[
            "--vcs",
            "--vcs-remote",
            "upstream",
            "--vcs-default-branch",
            "develop",
        ]);
        assert!(cli.vcs);
        assert_eq!(cli.vcs_remote.as_deref(), Some("upstream"));
        assert_eq!(cli.vcs_default_branch.as_deref(), Some("develop"));
        assert_eq!(cli.anchor().unwrap(), Anchor::Vcs);
    }
}
