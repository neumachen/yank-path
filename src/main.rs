//! `yank-path` binary entry point — the composition root.
//!
//! All business logic lives in the library crate. This file is intentionally
//! tiny: it parses CLI args, constructs the real collaborator implementations
//! (OS filesystem, walk-up Git detector, VCS info provider, arboard clipboard,
//! stdout sink), hands them to [`App`], and maps any [`YankError`] to a
//! distinct exit code defined by [`YankError::exit_code`].

use std::process::ExitCode;

use clap::Parser;

use yank_path::{
    App, ArboardClipboard, Cli, GitDirVcsInfoProvider, GitLsRemoteVerifier, OsFileSystem,
    StdoutSink, WalkUpGitRootDetector, YankError,
};

fn main() -> ExitCode {
    let cli = Cli::parse();

    let fs = OsFileSystem::new();
    let git_detector = WalkUpGitRootDetector::new();
    let vcs_provider = GitDirVcsInfoProvider::new();
    let verifier = GitLsRemoteVerifier::new();
    let mut clipboard = ArboardClipboard::new();
    let mut sink = StdoutSink;

    let mut app = App::new(
        &fs,
        &git_detector,
        &vcs_provider,
        &verifier,
        &mut clipboard,
        &mut sink,
    );

    match app.run(&cli) {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            report_error(&err);
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

/// Print a single, human-readable error line to stderr.
///
/// Kept separate from `main` so we have one obvious place to evolve the
/// error-reporting format (colour, structured output, etc.) without
/// touching wiring.
fn report_error(err: &YankError) {
    eprintln!("yank-path: {err}");
}
