//! VCS URL anchor integration tests for `yank-path`.
//!
//! These tests exercise `--vcs` URL generation using fake `.git/` directories.
//! No real git binary or network access is required (test 8 explicitly removes
//! git from PATH to verify graceful degradation).

mod common;

use std::fs;

use predicates::prelude::*;
use tempfile::tempdir;

use common::{canonical, fake_git_repo, touch, yp, SHA};

// ---------------------------------------------------------------------------
// 12. --vcs GitHub HTTPS renders blob permalink
// ---------------------------------------------------------------------------

#[test]
fn vcs_github_https_renders_blob_permalink() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    fake_git_repo(
        &repo,
        "https://github.com/user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    fs::create_dir(repo.join("src")).unwrap();
    touch(&repo.join("src"), "lib.rs");

    let expected = format!("https://github.com/user/repo/blob/{SHA}/src/lib.rs\n");

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "src/lib.rs"])
        .assert()
        .success()
        .stdout(expected);
}

// ---------------------------------------------------------------------------
// 13. --vcs GitLab SSH renders /-/blob permalink
// ---------------------------------------------------------------------------

#[test]
fn vcs_gitlab_ssh_renders_dash_blob() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    fake_git_repo(
        &repo,
        "git@gitlab.com:user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    touch(&repo, "README.md");

    let expected = format!("https://gitlab.com/user/repo/-/blob/{SHA}/README.md\n");

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "README.md"])
        .assert()
        .success()
        .stdout(expected);
}

// ---------------------------------------------------------------------------
// 14. --vcs Bitbucket HTTPS renders /src/ permalink
// ---------------------------------------------------------------------------

#[test]
fn vcs_bitbucket_https_renders_src() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    fake_git_repo(
        &repo,
        "https://bitbucket.org/user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    touch(&repo, "README.md");

    let expected = format!("https://bitbucket.org/user/repo/src/{SHA}/README.md\n");

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "README.md"])
        .assert()
        .success()
        .stdout(expected);
}

// ---------------------------------------------------------------------------
// 15. SSH remote is normalized to HTTPS URL
// ---------------------------------------------------------------------------

#[test]
fn vcs_ssh_remote_is_normalized_to_https() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    // SSH-style remote → should produce HTTPS URL
    fake_git_repo(
        &repo,
        "git@github.com:user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    touch(&repo, "file.txt");

    let expected = format!("https://github.com/user/repo/blob/{SHA}/file.txt\n");

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "file.txt"])
        .assert()
        .success()
        .stdout(expected);
}

// ---------------------------------------------------------------------------
// 16. --vcs-branch-fallback uses branch when SHA is unresolvable
// ---------------------------------------------------------------------------

#[test]
fn vcs_branch_fallback_when_no_sha() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    // No refs written → SHA unresolvable, but branch "main" can be used.
    fake_git_repo(
        &repo,
        "git@github.com:user/repo.git",
        "ref: refs/heads/main\n",
        &[], // no ref files
    );
    touch(&repo, "file.txt");

    yp(&repo)
        .args([
            "--print",
            "--no-copy",
            "--vcs",
            "--vcs-branch-fallback",
            "file.txt",
        ])
        .assert()
        .success()
        .stdout("https://github.com/user/repo/blob/main/file.txt\n");
}

// ---------------------------------------------------------------------------
// 17. --vcs without fallback errors when SHA unresolvable (exit code 12)
// ---------------------------------------------------------------------------

#[test]
fn vcs_no_sha_without_fallback_errors() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    // No refs written → SHA unresolvable, and no fallback allowed.
    fake_git_repo(
        &repo,
        "git@github.com:user/repo.git",
        "ref: refs/heads/main\n",
        &[], // no ref files
    );
    touch(&repo, "file.txt");

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "file.txt"])
        .assert()
        .failure()
        .code(12)
        .stderr(predicate::str::contains("--vcs-branch-fallback"));
}

// ---------------------------------------------------------------------------
// 18. --vcs conflicts with --absolute (clap exit code 2)
// ---------------------------------------------------------------------------

#[test]
fn vcs_conflicts_with_absolute_exits_two() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    fake_git_repo(
        &repo,
        "https://github.com/user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    touch(&repo, "a.txt");

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "--absolute", "a.txt"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::is_empty().not());
}

// ---------------------------------------------------------------------------
// 19. --vcs-verify with git off PATH → unverified but succeeds
// ---------------------------------------------------------------------------

#[test]
fn vcs_verify_with_git_off_path_is_unverified_but_succeeds() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    fake_git_repo(
        &repo,
        "https://github.com/user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    touch(&repo, "file.txt");

    let expected_url = format!("https://github.com/user/repo/blob/{SHA}/file.txt");

    yp(&repo)
        .env("PATH", "")
        .args(["--print", "--no-copy", "--vcs", "--vcs-verify", "file.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&expected_url))
        .stderr(predicate::str::contains("note: could not verify"))
        .stderr(predicate::str::contains("git not available"));
}
