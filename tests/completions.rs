//! Shell completion integration tests for `yank-path`.
//!
//! These tests exercise the `--completions` flag which generates shell
//! completion scripts and exits. Network-free, deterministic.

mod common;

use predicates::prelude::*;
use tempfile::tempdir;

use common::{canonical, yp};

// ---------------------------------------------------------------------------
// 20. --completions zsh emits #compdef script
// ---------------------------------------------------------------------------

#[test]
fn completions_zsh_emits_compdef_script() {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);

    yp(&cwd)
        .args(["--completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef yank-path"))
        .stdout(predicate::str::contains("--vcs"));
}

// ---------------------------------------------------------------------------
// 21. --completions bash emits script containing binary name
// ---------------------------------------------------------------------------

#[test]
fn completions_bash_emits_script() {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);

    yp(&cwd)
        .args(["--completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("yank-path"));
}
