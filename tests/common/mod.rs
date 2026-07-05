//! Shared test helpers for integration tests.
//!
//! These utilities are shared across the split test files (anchors.rs,
//! glob.rs, vcs.rs, completions.rs). This module is NOT compiled as a
//! standalone test binary because it lives in a subdirectory.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// Stable 40-hex SHA used across all VCS tests.
pub const SHA: &str = "abc1234567890123456789012345678901234567";

/// Canonicalise the tempdir so we compare against the same form that
/// `std::fs::canonicalize` produces inside the binary (e.g. on macOS where
/// `/tmp` is a symlink to `/private/tmp`).
pub fn canonical(dir: &TempDir) -> PathBuf {
    fs::canonicalize(dir.path()).expect("canonicalize tempdir")
}

/// Create an empty regular file at `dir/name`.
pub fn touch(dir: &Path, name: &str) {
    fs::write(dir.join(name), b"").unwrap_or_else(|e| panic!("touch {name}: {e}"));
}

/// Build a `Command` for the `yank-path` binary, with `current_dir` set to
/// `cwd` and `HOME` cleared. Tests that exercise the home anchor must
/// re-add `HOME` explicitly via `.env("HOME", …)`.
pub fn yp(cwd: &Path) -> Command {
    let mut cmd = Command::cargo_bin("yank-path").expect("binary should build");
    cmd.current_dir(cwd).env_remove("HOME");
    cmd
}

/// Create a fake `.git/` inside `root` so the walk-up detector finds the repo
/// and `GitDirVcsInfoProvider` can parse it — entirely offline, no `git`.
///
/// * `remote_url` becomes `[remote "origin"] url = <remote_url>`.
/// * `head` is written verbatim to `.git/HEAD` (e.g. `ref: refs/heads/main\n`).
/// * each `(ref_path, sha)` writes `.git/<ref_path>` with `<sha>\n`.
pub fn fake_git_repo(root: &Path, remote_url: &str, head: &str, refs: &[(&str, &str)]) {
    let git_dir = root.join(".git");
    fs::create_dir_all(&git_dir).unwrap();
    let config = format!("[remote \"origin\"]\n\turl = {remote_url}\n");
    fs::write(git_dir.join("config"), config).unwrap();
    fs::write(git_dir.join("HEAD"), head).unwrap();
    for (ref_path, sha) in refs {
        let full = git_dir.join(ref_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, format!("{sha}\n")).unwrap();
    }
}
