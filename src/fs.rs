//! `FileSystem` DI trait and the real OS-backed implementation.
//!
//! The trait isolates the resolve/render pipeline from `std::fs` so tests
//! can drive it with an in-memory fake.

use std::path::{Path, PathBuf};

use crate::error::YankError;

/// Filesystem abstraction injected throughout the pipeline.
///
/// Only the surface area the pipeline actually needs is exposed; this keeps
/// the trait small (Interface Segregation) and easy to fake in tests.
pub trait FileSystem {
    /// Current working directory.
    fn cwd(&self) -> Result<PathBuf, YankError>;

    /// User home directory (typically `$HOME`). `None` if unknown.
    fn home(&self) -> Option<PathBuf>;

    /// Whether `path` exists on disk.
    fn exists(&self, path: &Path) -> bool;

    /// Canonicalise `path` to an absolute, symlink-resolved form.
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, YankError>;

    /// Whether `path` is a directory.
    fn is_dir(&self, path: &Path) -> bool;

    /// Whether `path` is a regular file.
    fn is_file(&self, path: &Path) -> bool;
}

/// Real `FileSystem` implementation backed by `std::env` + `std::fs`.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsFileSystem;

impl OsFileSystem {
    pub fn new() -> Self {
        Self
    }
}

impl FileSystem for OsFileSystem {
    fn cwd(&self) -> Result<PathBuf, YankError> {
        std::env::current_dir().map_err(YankError::from)
    }

    fn home(&self) -> Option<PathBuf> {
        // `HOME` is the POSIX source of truth and is also honoured by most
        // shells on Windows under MSYS/Cygwin. We deliberately avoid pulling
        // in `dirs` for one variable.
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, YankError> {
        std::fs::canonicalize(path).map_err(YankError::from)
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}
