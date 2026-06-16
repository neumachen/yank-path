//! Single-level, files-only glob expansion.
//!
//! Phase 6 of the yank-path pipeline. Patterns are matched against the
//! current working directory only; recursive (`**`) and multi-segment (`/`)
//! patterns are rejected with distinct errors so users get actionable
//! feedback. Output is sorted lexicographically for deterministic CLI
//! behaviour, and duplicates are intentionally preserved (no dedup here).

use std::path::PathBuf;

use glob::glob;

use crate::error::YankError;
use crate::fs::FileSystem;

/// Expand a list of single-level glob patterns into matching file paths.
///
/// Rules:
/// * A pattern containing `**` is rejected with
///   [`YankError::RecursiveGlobRejected`] (checked **before** the `/`
///   check, so users get the more specific error).
/// * A pattern containing `/` is rejected with [`YankError::GlobHasSlash`].
/// * Otherwise the pattern is anchored to `fs.cwd()` and expanded with
///   the `glob` crate. Only regular files are kept (directories that match
///   the pattern are silently skipped). Individual entry I/O errors are
///   ignored — the only way to fail here is via the aggregate emptiness
///   check below.
/// * After processing all patterns, the aggregated paths are sorted
///   lexicographically. No deduplication is performed.
/// * If the aggregate is empty across **all** patterns, returns
///   [`YankError::GlobNoMatch`] with the original pattern list.
///
/// If `patterns` is empty, returns `Ok(vec![])` — "no patterns supplied"
/// is distinct from "patterns supplied but matched nothing" and the caller
/// (the CLI driver) decides what that means.
pub fn expand_globs(patterns: &[String], fs: &dyn FileSystem) -> Result<Vec<PathBuf>, YankError> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }

    let cwd = fs.cwd()?;
    let mut out: Vec<PathBuf> = Vec::new();

    for pattern in patterns {
        if pattern.contains("**") {
            return Err(YankError::RecursiveGlobRejected(pattern.clone()));
        }
        if pattern.contains('/') {
            return Err(YankError::GlobHasSlash(pattern.clone()));
        }

        let anchored = format!("{}/{}", cwd.display(), pattern);
        let paths = glob(&anchored).map_err(|e| {
            YankError::InvalidUsage(format!("invalid glob pattern '{pattern}': {e}"))
        })?;

        for entry in paths {
            // Ignore per-entry I/O errors (e.g. permission denied on a
            // single hit); they shouldn't poison the whole expansion.
            let path = match entry {
                Ok(p) => p,
                Err(_) => continue,
            };
            if fs.is_file(&path) {
                out.push(path);
            }
        }
    }

    if out.is_empty() {
        return Err(YankError::GlobNoMatch(patterns.to_vec()));
    }

    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::YankError;
    use crate::fs::{FileSystem, OsFileSystem};
    use std::fs as stdfs;
    use std::path::{Path, PathBuf};
    use tempfile::{tempdir, TempDir};

    /// Wraps `OsFileSystem` but overrides `cwd()` to point at a tempdir.
    /// Everything else delegates to the real OS — `glob` hits the real
    /// filesystem so a pure fake wouldn't help here.
    struct TempDirFs {
        inner: OsFileSystem,
        cwd: PathBuf,
    }

    impl TempDirFs {
        fn new(dir: &TempDir) -> Self {
            // Canonicalise so symlinked tempdirs (e.g. /tmp -> /private/tmp
            // on macOS) compare equal to glob's output.
            let cwd = stdfs::canonicalize(dir.path()).expect("canonicalize tempdir");
            Self {
                inner: OsFileSystem,
                cwd,
            }
        }
    }

    impl FileSystem for TempDirFs {
        fn cwd(&self) -> Result<PathBuf, YankError> {
            Ok(self.cwd.clone())
        }
        fn home(&self) -> Option<PathBuf> {
            self.inner.home()
        }
        fn exists(&self, path: &Path) -> bool {
            self.inner.exists(path)
        }
        fn canonicalize(&self, path: &Path) -> Result<PathBuf, YankError> {
            self.inner.canonicalize(path)
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.inner.is_dir(path)
        }
        fn is_file(&self, path: &Path) -> bool {
            self.inner.is_file(path)
        }
    }

    fn touch(dir: &Path, name: &str) {
        stdfs::write(dir.join(name), b"").expect("touch");
    }

    #[test]
    fn rejects_recursive_pattern() {
        let dir = tempdir().unwrap();
        let fs = TempDirFs::new(&dir);

        match expand_globs(&["**.rs".to_string()], &fs) {
            Err(YankError::RecursiveGlobRejected(p)) => assert_eq!(p, "**.rs"),
            other => panic!("expected RecursiveGlobRejected, got {other:?}"),
        }

        match expand_globs(&["**".to_string()], &fs) {
            Err(YankError::RecursiveGlobRejected(p)) => assert_eq!(p, "**"),
            other => panic!("expected RecursiveGlobRejected, got {other:?}"),
        }
    }

    #[test]
    fn rejects_pattern_with_slash() {
        let dir = tempdir().unwrap();
        let fs = TempDirFs::new(&dir);

        match expand_globs(&["src/*.rs".to_string()], &fs) {
            Err(YankError::GlobHasSlash(p)) => assert_eq!(p, "src/*.rs"),
            other => panic!("expected GlobHasSlash, got {other:?}"),
        }
    }

    #[test]
    fn recursive_check_takes_precedence_over_slash_check() {
        let dir = tempdir().unwrap();
        let fs = TempDirFs::new(&dir);

        match expand_globs(&["**/*.rs".to_string()], &fs) {
            Err(YankError::RecursiveGlobRejected(p)) => assert_eq!(p, "**/*.rs"),
            other => panic!("expected RecursiveGlobRejected, got {other:?}"),
        }
    }

    #[test]
    fn files_only_skips_directories() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "a.txt");
        touch(dir.path(), "b.txt");
        stdfs::create_dir(dir.path().join("c.txt")).expect("mkdir c.txt");

        let fs = TempDirFs::new(&dir);
        let got = expand_globs(&["*.txt".to_string()], &fs).expect("matches a + b");

        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn sorted_output() {
        let dir = tempdir().unwrap();
        // Create in reverse alphabetical order — filesystem ordering varies.
        for name in &["z.txt", "m.txt", "a.txt"] {
            touch(dir.path(), name);
        }

        let fs = TempDirFs::new(&dir);
        let got = expand_globs(&["*.txt".to_string()], &fs).expect("matches");
        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.txt", "m.txt", "z.txt"]);
    }

    #[test]
    fn no_dedup_across_patterns() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "a.txt");

        let fs = TempDirFs::new(&dir);
        let got =
            expand_globs(&["a.txt".to_string(), "a.txt".to_string()], &fs).expect("matches twice");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], got[1]);
    }

    #[test]
    fn empty_aggregate_returns_glob_no_match() {
        let dir = tempdir().unwrap();
        let fs = TempDirFs::new(&dir);

        match expand_globs(&["*.nonexistent".to_string()], &fs) {
            Err(YankError::GlobNoMatch(ps)) => {
                assert_eq!(ps, vec!["*.nonexistent".to_string()]);
            }
            other => panic!("expected GlobNoMatch, got {other:?}"),
        }
    }

    #[test]
    fn empty_patterns_slice_returns_empty_vec() {
        let dir = tempdir().unwrap();
        let fs = TempDirFs::new(&dir);
        let got = expand_globs(&[], &fs).expect("empty input -> empty output");
        assert!(got.is_empty());
    }

    #[test]
    fn multiple_patterns_aggregate_and_sort() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "a.txt");
        touch(dir.path(), "b.md");
        touch(dir.path(), "c.txt");

        let fs = TempDirFs::new(&dir);
        let got = expand_globs(&["*.md".to_string(), "*.txt".to_string()], &fs)
            .expect("matches across patterns");

        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        // Sorted lexicographically across the union, not grouped by pattern.
        assert_eq!(names, vec!["a.txt", "b.md", "c.txt"]);
    }
}
