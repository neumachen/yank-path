//! Glob mode integration tests for `yank-path`.
//!
//! Coverage:
//! * `--glob` success → sorted matches.
//! * `--glob` no-match → non-zero exit (GlobNoMatch = code 6).
//! * `--glob` with `**` → rejected (RecursiveGlobRejected = code 4).

mod common;

use predicates::prelude::*;
use tempfile::tempdir;

use common::{canonical, touch, yp};

// ---------------------------------------------------------------------------
// 6. --glob success → sorted matches
// ---------------------------------------------------------------------------

#[test]
fn glob_matches_files_and_outputs_sorted() {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    // Create out of order to prove output is sorted.
    for name in &["z.txt", "m.txt", "a.txt"] {
        touch(&cwd, name);
    }
    // A non-matching file to make sure we filter on the pattern.
    touch(&cwd, "ignore.md");

    let base = cwd.file_name().unwrap().to_string_lossy().into_owned();
    let expected = format!("{base}/a.txt\n{base}/m.txt\n{base}/z.txt\n");

    yp(&cwd)
        .args(["--print", "--no-copy", "--from", "base", "--glob", "*.txt"])
        .assert()
        .success()
        .stdout(expected);
}

// ---------------------------------------------------------------------------
// 7. --glob no-match → non-zero exit (GlobNoMatch = code 6)
// ---------------------------------------------------------------------------

#[test]
fn glob_with_no_matches_exits_non_zero() {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    // Empty directory.

    yp(&cwd)
        .args(["--print", "--no-copy", "--glob", "*.nonexistent_extension"])
        .assert()
        .failure()
        .code(6)
        .stderr(predicate::str::contains("no files matched"));
}

// ---------------------------------------------------------------------------
// 8. --glob with `**` → rejected (RecursiveGlobRejected = code 4)
// ---------------------------------------------------------------------------

#[test]
fn glob_with_double_star_is_rejected() {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    touch(&cwd, "a.txt");

    yp(&cwd)
        .args(["--print", "--no-copy", "--glob", "**/*.txt"])
        .assert()
        .failure()
        .code(4)
        .stderr(predicate::str::contains("recursive glob rejected"));
}
