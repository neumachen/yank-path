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

// ---------------------------------------------------------------------------
// 20. --vcs with multiple files yields newline-joined URLs sharing the same SHA
// ---------------------------------------------------------------------------

#[test]
fn vcs_multiple_files_are_newline_joined() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    fake_git_repo(
        &repo,
        "https://github.com/user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    touch(&repo, "a.txt");
    touch(&repo, "b.txt");

    let expected = format!(
        "https://github.com/user/repo/blob/{SHA}/a.txt\nhttps://github.com/user/repo/blob/{SHA}/b.txt\n"
    );

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "a.txt", "b.txt"])
        .assert()
        .success()
        .stdout(expected);
}

// ---------------------------------------------------------------------------
// 21. --vcs with multiple files across subdirectories preserves repo-relative paths
// ---------------------------------------------------------------------------

#[test]
fn vcs_multiple_files_across_subdirectories() {
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
    touch(&repo, "README.md");

    let expected = format!(
        "https://github.com/user/repo/blob/{SHA}/src/lib.rs\nhttps://github.com/user/repo/blob/{SHA}/README.md\n"
    );

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "src/lib.rs", "README.md"])
        .assert()
        .success()
        .stdout(expected);
}

// ---------------------------------------------------------------------------
// 22. --vcs with --vcs-branch-fallback for multiple files uses branch for all
// ---------------------------------------------------------------------------

#[test]
fn vcs_multiple_files_with_branch_fallback() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    // SSH remote, no refs written (SHA unresolvable) → branch fallback.
    fake_git_repo(
        &repo,
        "git@github.com:user/repo.git",
        "ref: refs/heads/main\n",
        &[], // no ref files → SHA unresolvable
    );
    touch(&repo, "a.txt");
    touch(&repo, "b.txt");

    yp(&repo)
        .args([
            "--print",
            "--no-copy",
            "--vcs",
            "--vcs-branch-fallback",
            "a.txt",
            "b.txt",
        ])
        .assert()
        .success()
        .stdout("https://github.com/user/repo/blob/main/a.txt\nhttps://github.com/user/repo/blob/main/b.txt\n");
}

// ---------------------------------------------------------------------------
// 23. --vcs with one missing file aborts before touching output (exit 2)
// ---------------------------------------------------------------------------

/// When one operand file does not exist locally, the strict all-or-nothing
/// resolution in `resolve_operands` aborts with `NotFound` (exit 2) BEFORE
/// any output is emitted. This directly answers the question "what happens
/// when one file does not exist locally?".
#[test]
fn vcs_multiple_files_one_missing_aborts() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    fake_git_repo(
        &repo,
        "https://github.com/user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    // Only touch a.txt; missing.txt is deliberately not created.
    touch(&repo, "a.txt");

    yp(&repo)
        .args(["--print", "--no-copy", "--vcs", "a.txt", "missing.txt"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty());
}

// ---------------------------------------------------------------------------
// 24. --vcs-verify with multiple files emits ONE verification note
// ---------------------------------------------------------------------------

/// With `--vcs-verify` and git off PATH, verification produces an `Unverified`
/// outcome. Crucially, the verification runs ONCE at the repo level (not
/// per-file), so even with two files we should see exactly one "could not
/// verify" note on stderr — NOT two.
#[test]
fn vcs_verify_multiple_files_unverified_but_succeeds() {
    let dir = tempdir().unwrap();
    let repo = canonical(&dir);
    fake_git_repo(
        &repo,
        "https://github.com/user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    touch(&repo, "a.txt");
    touch(&repo, "b.txt");

    let expected_url_a = format!("https://github.com/user/repo/blob/{SHA}/a.txt");
    let expected_url_b = format!("https://github.com/user/repo/blob/{SHA}/b.txt");

    let output = yp(&repo)
        .env("PATH", "")
        .args([
            "--print",
            "--no-copy",
            "--vcs",
            "--vcs-verify",
            "a.txt",
            "b.txt",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(&expected_url_a))
        .stdout(predicate::str::contains(&expected_url_b))
        .stderr(predicate::str::contains("note: could not verify"))
        .stderr(predicate::str::contains("git not available"))
        .get_output()
        .clone();

    // Verify the "could not verify" note appears exactly ONCE (repo-level,
    // not per-file).
    let stderr = String::from_utf8_lossy(&output.stderr);
    let count = stderr.matches("could not verify").count();
    assert_eq!(
        count, 1,
        "expected exactly 1 'could not verify' note for the whole run, got {count}"
    );
}
