//! Application pipeline (Phase 8).
//!
//! [`App`] is the orchestrator that glues the injected collaborators —
//! filesystem, git-root detector, vcs-info provider, clipboard, output sink —
//! together and runs the resolve → glob → render → emit pipeline driven by a
//! parsed [`Cli`].
//!
//! The composition root in `src/main.rs` constructs real implementations
//! and hands them to `App::run`. Tests construct in-memory fakes (see
//! [`crate::clipboard::FakeClipboard`], [`crate::clipboard::BufferSink`]
//! and the per-test `MockFileSystem` defined below) and drive the same
//! `run` method, which is the only path that exercises the pipeline.

use crate::anchor::{render_with, AnchorRenderer, RenderContext, VcsRenderer};
use crate::cli::Cli;
use crate::clipboard::{Clipboard, OutputSink};
use crate::error::YankError;
use crate::fs::FileSystem;
use crate::gitroot::GitRootDetector;
use crate::glob::expand_globs;
use crate::resolve::resolve_operands;
use crate::vcs::{RemoteVerifier, VcsInfoProvider, VerifyOutcome};

use std::path::PathBuf;

/// Pipeline orchestrator.
///
/// All collaborators are injected as trait objects. Clipboard and output
/// sink are `&mut` because they mutate (write text); filesystem,
/// git-root detector, VCS info provider, and remote verifier are read-only.
pub struct App<'a> {
    fs: &'a dyn FileSystem,
    git_detector: &'a dyn GitRootDetector,
    vcs_provider: &'a dyn VcsInfoProvider,
    verifier: &'a dyn RemoteVerifier,
    clipboard: &'a mut dyn Clipboard,
    sink: &'a mut dyn OutputSink,
}

impl<'a> App<'a> {
    /// Construct an `App` from injected collaborators.
    pub fn new(
        fs: &'a dyn FileSystem,
        git_detector: &'a dyn GitRootDetector,
        vcs_provider: &'a dyn VcsInfoProvider,
        verifier: &'a dyn RemoteVerifier,
        clipboard: &'a mut dyn Clipboard,
        sink: &'a mut dyn OutputSink,
    ) -> Self {
        Self {
            fs,
            git_detector,
            vcs_provider,
            verifier,
            clipboard,
            sink,
        }
    }

    /// Execute the full yank-path pipeline for a parsed [`Cli`].
    ///
    /// Returns the process exit code on success (`0`). On error returns the
    /// underlying [`YankError`]; the caller (`main`) maps it to a distinct
    /// non-zero exit code via [`YankError::exit_code`].
    ///
    /// Pipeline:
    /// 1. Resolve the anchor (`Cli::anchor`).
    /// 2. Gather operands: positional paths, then glob matches. If both are
    ///    empty, default to `["."]`. `--glob` (when non-empty) drives
    ///    matches and the `.` default does **not** apply.
    /// 3. Strict all-or-nothing existence check via `resolve_operands`. Any
    ///    failure aborts before the clipboard is touched.
    /// 4. Build a [`RenderContext`] and render each resolved path under the
    ///    chosen anchor, joining results with `\n`.
    /// 5. Emit:
    ///    * `--no-copy` + `--print`: write to sink only, never touch
    ///      clipboard.
    ///    * `--no-copy` only: silent success (no output, no clipboard).
    ///    * neither: copy to clipboard. If the backend is unavailable
    ///      (headless) or `set_text` reports `ClipboardUnavailable`, fall
    ///      back to writing to the sink. With `--print` also set, ensure
    ///      we don't double-write.
    ///    * `--print` plus working clipboard: copy *and* write to sink.
    /// 6. Flush the sink and return `Ok(0)`.
    pub fn run(&mut self, cli: &Cli) -> Result<i32, YankError> {
        let anchor = cli.anchor()?;

        // --- Gather operands -------------------------------------------------
        // Order: positional paths first, then glob matches. Duplicates kept.
        let mut operands: Vec<PathBuf> = cli.paths.clone();
        let glob_matches = expand_globs(&cli.glob, self.fs)?;
        operands.extend(glob_matches);

        // Default to `.` only when BOTH positional paths and `--glob` are
        // empty. (When `--glob` was given but matched zero files, the
        // glob stage above will have already errored with `GlobNoMatch`.)
        if operands.is_empty() && cli.glob.is_empty() {
            operands.push(PathBuf::from("."));
        }

        // --- Resolve (strict, all-or-nothing) --------------------------------
        let resolved = resolve_operands(&operands, self.fs)?;

        // --- Render ----------------------------------------------------------
        let cwd = self.fs.cwd()?;
        let home = self.fs.home();
        let git_root = self.git_detector.find_root(&cwd);

        let ctx = RenderContext {
            cwd,
            home,
            git_root: git_root.clone(),
            fs: self.fs,
        };

        // For VCS anchor, we need to resolve VcsInfo first and render with VcsRenderer.
        // We also emit offline warnings for suspicious local state, and optionally
        // verify the ref exists on the remote.
        let (rendered, stderr_lines) = if anchor == crate::anchor::Anchor::Vcs {
            self.render_vcs(&resolved, &ctx, cli, git_root.as_ref())?
        } else {
            let mut rendered: Vec<String> = Vec::with_capacity(resolved.len());
            for path in &resolved {
                rendered.push(render_with(&anchor, path, &ctx)?);
            }
            (rendered, Vec::new())
        };
        let joined = rendered.join("\n");

        // --- Emit ------------------------------------------------------------
        // `wrote_to_sink` tracks whether we already wrote `joined` to the
        // sink in this run, so we never double-emit when both headless
        // fallback and `--print` apply.
        let mut wrote_to_sink = false;

        if cli.no_copy {
            // Never touch the clipboard. Print only if asked to.
            if cli.print {
                self.sink.write_line(&joined)?;
                wrote_to_sink = true;
            }
        } else if !self.clipboard.is_available() {
            // Headless: fall back to stdout instead of hard-failing.
            self.sink.write_line(&joined)?;
            wrote_to_sink = true;
        } else {
            // Try to copy. If the backend reports it's unavailable at
            // write-time (race / transient), fall back to the sink
            // rather than failing the run.
            match self.clipboard.set_text(&joined) {
                Ok(()) => {}
                Err(YankError::ClipboardUnavailable(_)) => {
                    self.sink.write_line(&joined)?;
                    wrote_to_sink = true;
                }
                Err(other) => return Err(other),
            }
        }

        // `--print` always writes to the sink — unless we already wrote
        // via the headless fallback above (which would duplicate output).
        if cli.print && !wrote_to_sink {
            self.sink.write_line(&joined)?;
        }

        // Emit VCS stderr lines (offline warning and/or verify output) AFTER
        // output, so they don't pollute stdout/clipboard content.
        for line in &stderr_lines {
            eprintln!("{line}");
        }

        self.sink.flush()?;
        Ok(0)
    }

    /// Render paths using the VCS URL anchor.
    ///
    /// Returns the rendered strings and a list of stderr lines to emit (offline
    /// warnings and/or verification output). The offline warning is emitted
    /// when local state suggests the generated URLs might not work (detached
    /// HEAD, no upstream, or ahead of remote). When `--vcs-verify` is enabled,
    /// we also check if the ref exists on the remote.
    fn render_vcs(
        &self,
        resolved: &[PathBuf],
        ctx: &RenderContext<'_>,
        cli: &Cli,
        git_root: Option<&PathBuf>,
    ) -> Result<(Vec<String>, Vec<String>), YankError> {
        let git_root = git_root.ok_or_else(|| YankError::Vcs("not in a repository".to_string()))?;

        let remote = cli.vcs_remote.as_deref().unwrap_or("origin");
        let vcs_info = self.vcs_provider.info(git_root, remote)?;

        // Build offline warning message if local state is suspicious
        let mut stderr_lines = Vec::new();
        if let Some(warning) = build_vcs_warning(&vcs_info, remote) {
            stderr_lines.push(warning);
        }

        // Construct the renderer
        let renderer = VcsRenderer::new(
            vcs_info.clone(),
            cli.vcs_default_branch.clone(),
            cli.vcs_branch_fallback,
        );

        let mut rendered: Vec<String> = Vec::with_capacity(resolved.len());
        for path in resolved {
            rendered.push(renderer.render(path, ctx)?);
        }

        // Opt-in remote verification (--vcs-verify)
        if cli.vcs_verify {
            // Determine the ref that was used in the URL — same logic as VcsRenderer
            let git_ref = if let Some(ref sha) = vcs_info.sha {
                sha.clone()
            } else if cli.vcs_branch_fallback {
                cli.vcs_default_branch
                    .clone()
                    .or(vcs_info.branch.clone())
                    .unwrap_or_else(|| "main".to_string())
            } else {
                // No SHA and no branch fallback — this would have errored in render(),
                // but be defensive here
                "main".to_string()
            };

            let outcome = self.verifier.verify(git_root, remote, &git_ref);
            match outcome {
                VerifyOutcome::Present => {
                    // Confirmed on remote — no message
                }
                VerifyOutcome::Absent => {
                    let ref_short = if git_ref.len() == 40 {
                        &git_ref[..7]
                    } else {
                        &git_ref
                    };
                    stderr_lines.push(format!(
                        "yank-path: warning: {ref_short} not found on remote '{remote}'"
                    ));
                }
                VerifyOutcome::Unverified(reason) => {
                    stderr_lines.push(format!(
                        "yank-path: note: could not verify against remote '{remote}' ({reason})"
                    ));
                }
            }
        }

        Ok((rendered, stderr_lines))
    }
}

/// Build a warning message for VCS state that might cause URL issues.
///
/// Returns `Some(warning)` if any of these conditions hold:
/// - Detached HEAD (commit might be orphaned)
/// - No upstream configured (branch might not exist on remote)
/// - Local ahead of remote (commits might not be pushed)
fn build_vcs_warning(info: &crate::vcs::VcsInfo, remote: &str) -> Option<String> {
    let sha_short = info
        .sha
        .as_ref()
        .map(|s| &s[..7.min(s.len())])
        .unwrap_or("unknown");

    let mut reasons = Vec::new();

    if info.detached {
        reasons.push("detached HEAD");
    }
    if !info.has_upstream && info.branch.is_some() {
        reasons.push("no upstream configured");
    }
    if info.ahead_of_remote {
        reasons.push("local commits not pushed");
    }

    if reasons.is_empty() {
        return None;
    }

    let reason_str = reasons.join(", ");
    Some(format!(
        "yank-path: warning: commit {sha_short} may not exist on remote '{remote}' ({reason_str})"
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::Anchor;
    use crate::cli::Cli;
    use crate::clipboard::{BufferSink, FakeClipboard};
    use crate::error::YankError;
    use crate::fs::FileSystem;
    use crate::gitroot::GitRootDetector;

    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    // -----------------------------------------------------------------------
    // Test-only fakes (kept local to the test module — never exported).
    // -----------------------------------------------------------------------

    /// Minimal in-memory `FileSystem` used by App tests.
    ///
    /// Behaves like the mocks in `resolve::tests` and `anchor::test_support`
    /// but is intentionally redefined here so test-only types stay out of
    /// the public crate surface.
    struct MemFs {
        cwd: PathBuf,
        home: Option<PathBuf>,
        existing: RefCell<HashSet<PathBuf>>,
        dirs: RefCell<HashSet<PathBuf>>,
    }

    impl MemFs {
        fn new(cwd: impl Into<PathBuf>) -> Self {
            Self {
                cwd: cwd.into(),
                home: None,
                existing: RefCell::new(HashSet::new()),
                dirs: RefCell::new(HashSet::new()),
            }
        }
        fn with_home(mut self, home: impl Into<PathBuf>) -> Self {
            self.home = Some(home.into());
            self
        }
        fn with_file(self, p: impl Into<PathBuf>) -> Self {
            self.existing.borrow_mut().insert(p.into());
            self
        }
        fn with_dir(self, p: impl Into<PathBuf>) -> Self {
            let p: PathBuf = p.into();
            self.existing.borrow_mut().insert(p.clone());
            self.dirs.borrow_mut().insert(p);
            self
        }
    }

    impl FileSystem for MemFs {
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

    /// A `GitRootDetector` that always returns `None` — fine for tests
    /// that don't exercise `--from git`.
    struct NoGit;
    impl GitRootDetector for NoGit {
        fn find_root(&self, _start: &Path) -> Option<PathBuf> {
            None
        }
    }

    /// A `VcsInfoProvider` that always errors — fine for tests that don't
    /// exercise `--vcs`.
    struct NoVcs;
    impl VcsInfoProvider for NoVcs {
        fn info(&self, _git_root: &Path, _remote: &str) -> Result<crate::vcs::VcsInfo, YankError> {
            Err(YankError::Vcs("VcsInfoProvider not configured".to_string()))
        }
    }

    /// A `RemoteVerifier` that always returns Present — fine for tests that
    /// don't exercise `--vcs-verify`.
    struct NoVerify;
    impl RemoteVerifier for NoVerify {
        fn verify(&self, _git_root: &Path, _remote: &str, _git_ref: &str) -> VerifyOutcome {
            VerifyOutcome::Present
        }
    }

    /// Build a default-ish `Cli` value programmatically so tests don't have
    /// to fight clap.
    fn cli_with(paths: Vec<PathBuf>) -> Cli {
        Cli {
            paths,
            from: None,
            relative_to: None,
            absolute: false,
            glob: vec![],
            print: false,
            no_copy: false,
            vcs: false,
            vcs_remote: None,
            vcs_default_branch: None,
            vcs_branch_fallback: false,
            vcs_verify: false,
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn default_home_anchor_copies_to_clipboard() {
        // Cwd `/home/u/proj`, home `/home/u`, no args → operand defaults to
        // `.`, rendered under Home anchor as `~/proj`, copied to clipboard.
        let fs = MemFs::new("/home/u/proj")
            .with_home("/home/u")
            .with_dir(PathBuf::from("/home/u/proj"));
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        let cli = cli_with(vec![]);
        let code = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(clip.contents().as_deref(), Some("~/proj"));
        assert!(sink.lines.is_empty(), "sink should be untouched");
    }

    #[test]
    fn print_writes_to_sink_and_clipboard() {
        let fs = MemFs::new("/home/u/proj")
            .with_home("/home/u")
            .with_dir(PathBuf::from("/home/u/proj"));
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        let mut cli = cli_with(vec![]);
        cli.print = true;
        let code = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(clip.contents().as_deref(), Some("~/proj"));
        assert_eq!(sink.joined(), "~/proj");
    }

    #[test]
    fn no_copy_with_print_writes_to_sink_only() {
        let fs = MemFs::new("/home/u/proj")
            .with_home("/home/u")
            .with_dir(PathBuf::from("/home/u/proj"));
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        let mut cli = cli_with(vec![]);
        cli.no_copy = true;
        cli.print = true;
        let code = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(
            clip.contents(),
            None,
            "clipboard must not be touched under --no-copy"
        );
        assert_eq!(sink.joined(), "~/proj");
    }

    #[test]
    fn no_copy_without_print_is_silent_success() {
        let fs = MemFs::new("/home/u/proj")
            .with_home("/home/u")
            .with_dir(PathBuf::from("/home/u/proj"));
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        let mut cli = cli_with(vec![]);
        cli.no_copy = true;
        let code = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(clip.contents(), None);
        assert!(sink.lines.is_empty());
    }

    #[test]
    fn headless_clipboard_falls_back_to_stdout() {
        let fs = MemFs::new("/home/u/proj")
            .with_home("/home/u")
            .with_dir(PathBuf::from("/home/u/proj"));
        let git = NoGit;
        let mut clip = FakeClipboard::new_unavailable();
        let mut sink = BufferSink::new();

        let cli = cli_with(vec![]);
        let code = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(clip.contents(), None, "headless clipboard untouched");
        assert_eq!(sink.joined(), "~/proj");
    }

    #[test]
    fn headless_plus_print_writes_only_once() {
        let fs = MemFs::new("/home/u/proj")
            .with_home("/home/u")
            .with_dir(PathBuf::from("/home/u/proj"));
        let git = NoGit;
        let mut clip = FakeClipboard::new_unavailable();
        let mut sink = BufferSink::new();

        let mut cli = cli_with(vec![]);
        cli.print = true;
        let code = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(sink.lines.len(), 1, "must not double-emit");
        assert_eq!(sink.joined(), "~/proj");
    }

    #[test]
    fn multiple_operands_are_newline_joined_in_order() {
        let fs = MemFs::new("/home/u/proj")
            .with_home("/home/u")
            .with_file(PathBuf::from("/home/u/proj/a.txt"))
            .with_file(PathBuf::from("/home/u/proj/b.txt"));
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        let cli = cli_with(vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")]);
        let code = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(
            clip.contents().as_deref(),
            Some("~/proj/a.txt\n~/proj/b.txt")
        );
    }

    #[test]
    fn missing_operand_aborts_with_not_found_and_clipboard_untouched() {
        let fs = MemFs::new("/home/u/proj")
            .with_home("/home/u")
            .with_file(PathBuf::from("/home/u/proj/a.txt"));
        // `missing.txt` is deliberately not registered.
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        let cli = cli_with(vec![PathBuf::from("a.txt"), PathBuf::from("missing.txt")]);
        let err = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap_err();

        match err {
            YankError::NotFound(p) => assert_eq!(p, PathBuf::from("missing.txt")),
            other => panic!("expected NotFound, got {other:?}"),
        }
        assert_eq!(
            clip.contents(),
            None,
            "clipboard must be untouched on validation error"
        );
        assert!(
            sink.lines.is_empty(),
            "sink must be untouched on validation error"
        );
    }

    #[test]
    fn glob_no_match_surfaces_glob_no_match_error() {
        // Use the real OS filesystem with a tempdir-style cwd that has no
        // matching files. We rely on `expand_globs` aborting before we ever
        // hit the renderer.
        let tmp = tempfile::tempdir().unwrap();
        let cwd = std::fs::canonicalize(tmp.path()).unwrap();

        // Empty cwd → pattern won't match anything.
        struct RealCwdFs {
            cwd: PathBuf,
        }
        impl FileSystem for RealCwdFs {
            fn cwd(&self) -> Result<PathBuf, YankError> {
                Ok(self.cwd.clone())
            }
            fn home(&self) -> Option<PathBuf> {
                None
            }
            fn exists(&self, p: &Path) -> bool {
                p.exists()
            }
            fn canonicalize(&self, p: &Path) -> Result<PathBuf, YankError> {
                std::fs::canonicalize(p).map_err(YankError::from)
            }
            fn is_dir(&self, p: &Path) -> bool {
                p.is_dir()
            }
            fn is_file(&self, p: &Path) -> bool {
                p.is_file()
            }
        }

        let fs = RealCwdFs { cwd };
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        let mut cli = cli_with(vec![]);
        cli.glob = vec!["*.nonexistent_ext".to_string()];

        let err = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap_err();
        match err {
            YankError::GlobNoMatch(patterns) => {
                assert_eq!(patterns, vec!["*.nonexistent_ext".to_string()]);
            }
            other => panic!("expected GlobNoMatch, got {other:?}"),
        }
        assert_eq!(clip.contents(), None);
        assert!(sink.lines.is_empty());
    }

    #[test]
    fn conflicting_anchor_options_propagate() {
        let fs = MemFs::new("/home/u/proj").with_home("/home/u");
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        // Bypass clap and set two anchors at once.
        let cli = Cli {
            paths: vec![],
            from: Some(crate::cli::FromAnchor::Home),
            relative_to: None,
            absolute: true,
            glob: vec![],
            print: false,
            no_copy: false,
            vcs: false,
            vcs_remote: None,
            vcs_default_branch: None,
            vcs_branch_fallback: false,
            vcs_verify: false,
        };
        let err = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap_err();
        assert!(matches!(err, YankError::ConflictingAnchors));
        assert_eq!(clip.contents(), None);
    }

    #[test]
    fn absolute_anchor_renders_canonical_paths() {
        let fs = MemFs::new("/home/u/proj").with_file(PathBuf::from("/home/u/proj/a.txt"));
        let git = NoGit;
        let mut clip = FakeClipboard::new_available();
        let mut sink = BufferSink::new();

        let mut cli = cli_with(vec![PathBuf::from("a.txt")]);
        cli.absolute = true;
        let code = App::new(&fs, &git, &NoVcs, &NoVerify, &mut clip, &mut sink)
            .run(&cli)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(clip.contents().as_deref(), Some("/home/u/proj/a.txt"));
        // Sanity: anchor was resolved to Absolute, not Home.
        assert_eq!(cli.anchor().unwrap(), Anchor::Absolute);
    }
}
