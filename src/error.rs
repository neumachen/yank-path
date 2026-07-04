//! Error types for the yank-path CLI.
//!
//! Each variant maps to a distinct process exit code in `main`, so callers
//! (and tests) can distinguish failure categories.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Errors produced by the yank-path pipeline.
///
/// Variants are mapped to distinct non-zero exit codes by [`YankError::exit_code`].
#[derive(Debug)]
pub enum YankError {
    /// A path operand did not exist on disk (strict validation).
    NotFound(PathBuf),
    /// `--from git` was requested but no `.git` ancestor was found.
    NotInRepo,
    /// A `--glob` pattern contained `**` (recursive globs are rejected in v1).
    RecursiveGlobRejected(String),
    /// A `--glob` pattern contained `/` (single-level only).
    GlobHasSlash(String),
    /// One or more `--glob` patterns produced zero matches.
    GlobNoMatch(Vec<String>),
    /// Conflicting anchor options were supplied (mutual exclusion).
    ConflictingAnchors,
    /// The system clipboard backend could not be initialised.
    ClipboardUnavailable(String),
    /// Generic I/O error wrapping `std::io::Error`.
    Io(io::Error),
    /// Catch-all for misuse not covered above.
    InvalidUsage(String),
    /// VCS operation failed (e.g. no remotes, unsupported host).
    Vcs(String),
}

impl YankError {
    /// Process exit code corresponding to this error category.
    ///
    /// Codes are stable and distinct so integration tests and shell scripts
    /// can branch on them.
    pub fn exit_code(&self) -> i32 {
        match self {
            YankError::NotFound(_) => 2,
            YankError::NotInRepo => 3,
            YankError::RecursiveGlobRejected(_) => 4,
            YankError::GlobHasSlash(_) => 5,
            YankError::GlobNoMatch(_) => 6,
            YankError::ConflictingAnchors => 7,
            YankError::ClipboardUnavailable(_) => 8,
            YankError::Io(_) => 10,
            YankError::InvalidUsage(_) => 11,
            YankError::Vcs(_) => 12,
        }
    }
}

impl fmt::Display for YankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            YankError::NotFound(p) => write!(f, "path does not exist: {}", p.display()),
            YankError::NotInRepo => write!(f, "not inside a Git repository (no .git ancestor)"),
            YankError::RecursiveGlobRejected(p) => write!(
                f,
                "recursive glob rejected (contains '**'): {p}; v1 supports single-level globs only"
            ),
            YankError::GlobHasSlash(p) => write!(
                f,
                "glob may not contain '/': {p}; single-level patterns only"
            ),
            YankError::GlobNoMatch(patterns) => {
                write!(
                    f,
                    "no files matched glob pattern(s): {}",
                    patterns.join(", ")
                )
            }
            YankError::ConflictingAnchors => write!(
                f,
                "--from, --absolute and --relative-to are mutually exclusive"
            ),
            YankError::ClipboardUnavailable(msg) => {
                write!(f, "clipboard backend unavailable: {msg}")
            }
            YankError::Io(e) => write!(f, "I/O error: {e}"),
            YankError::InvalidUsage(msg) => write!(f, "invalid usage: {msg}"),
            YankError::Vcs(msg) => write!(f, "VCS error: {msg}"),
        }
    }
}

impl std::error::Error for YankError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            YankError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for YankError {
    fn from(value: io::Error) -> Self {
        YankError::Io(value)
    }
}
