//! End-to-end CLI integration tests for `yank-path`.
//!
//! These exercise the real binary via `assert_cmd`, with stable cwd and
//! `$HOME` overrides per test, and `--print --no-copy` so we can assert
//! against stdout without touching a real system clipboard.
//!
//! Coverage:
//! * Default behaviour (`.` operand, home anchor).
//! * Every `--from` mode (`home`, `base`, `parent`, `git`).
//! * `--absolute` and `--relative-to <PATH>` anchors.
//! * Multiple operands → newline-joined, order preserved.
//! * `--glob` success, no-match (exit code 6), and recursive `**` rejection
//!   (exit code 4).
//! * Missing-path strict failure (exit code 2).
//! * `--print` flag emits to stdout; `--no-copy` keeps the clipboard
//!   untouched (combined with `--print` for testability).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::{tempdir, TempDir};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Canonicalise the tempdir so we compare against the same form that
/// `std::fs::canonicalize` produces inside the binary (e.g. on macOS where
/// `/tmp` is a symlink to `/private/tmp`).
fn canonical(dir: &TempDir) -> PathBuf {
    fs::canonicalize(dir.path()).expect("canonicalize tempdir")
}

/// Create an empty regular file at `dir/name`.
fn touch(dir: &Path, name: &str) {
    fs::write(dir.join(name), b"").unwrap_or_else(|e| panic!("touch {name}: {e}"));
}

/// Build a `Command` for the `yank-path` binary, with `current_dir` set to
/// `cwd` and `HOME` cleared. Tests that exercise the home anchor must
/// re-add `HOME` explicitly via `.env("HOME", …)`.
fn yp(cwd: &Path) -> Command {
    let mut cmd = Command::cargo_bin("yank-path").expect("binary should build");
    cmd.current_dir(cwd).env_remove("HOME");
    cmd
}

// ---------------------------------------------------------------------------
// 1. Default behaviour: no args → `.` operand → home-anchored cwd
// ---------------------------------------------------------------------------

#[test]
fn default_no_args_renders_home_anchored_cwd() {
    // Layout: $HOME = <tmp>, cwd = <tmp>/proj  → expected stdout: "~/proj"
    let home = tempdir().unwrap();
    let home_canon = canonical(&home);
    let proj = home_canon.join("proj");
    fs::create_dir(&proj).unwrap();

    yp(&proj)
        .env("HOME", &home_canon)
        .args(["--print", "--no-copy"])
        .assert()
        .success()
        .stdout("~/proj\n");
}

// ---------------------------------------------------------------------------
// 2. Each --from mode
// ---------------------------------------------------------------------------

#[test]
fn from_home_explicit_renders_tilde_prefixed_path() {
    let home = tempdir().unwrap();
    let home_canon = canonical(&home);
    let proj = home_canon.join("proj");
    fs::create_dir(&proj).unwrap();
    touch(&proj, "README.md");

    yp(&proj)
        .env("HOME", &home_canon)
        .args(["--print", "--no-copy", "--from", "home", "README.md"])
        .assert()
        .success()
        .stdout("~/proj/README.md\n");
}

#[test]
fn from_base_renders_basename_anchored_path() {
    // base anchor: leading segment is the cwd basename ("proj").
    let root = tempdir().unwrap();
    let proj = canonical(&root).join("proj");
    fs::create_dir(&proj).unwrap();
    touch(&proj, "lib.rs");

    yp(&proj)
        .args(["--print", "--no-copy", "--from", "base", "lib.rs"])
        .assert()
        .success()
        .stdout("proj/lib.rs\n");
}

#[test]
fn from_base_alias_basename_works() {
    let root = tempdir().unwrap();
    let proj = canonical(&root).join("proj");
    fs::create_dir(&proj).unwrap();
    touch(&proj, "lib.rs");

    yp(&proj)
        .args(["--print", "--no-copy", "--from", "basename", "lib.rs"])
        .assert()
        .success()
        .stdout("proj/lib.rs\n");
}

#[test]
fn from_parent_renders_parent_slash_cwd_anchored_path() {
    // parent anchor: leading segments are <parent-name>/<cwd-name>.
    let root = tempdir().unwrap();
    let projects = canonical(&root).join("projects");
    let repo = projects.join("example-repo");
    fs::create_dir_all(&repo).unwrap();
    touch(&repo, "README.md");

    yp(&repo)
        .args(["--print", "--no-copy", "--from", "parent", "README.md"])
        .assert()
        .success()
        .stdout("projects/example-repo/README.md\n");
}

#[test]
fn from_parent_alias_dirname_works() {
    let root = tempdir().unwrap();
    let projects = canonical(&root).join("projects");
    let repo = projects.join("example-repo");
    fs::create_dir_all(&repo).unwrap();
    touch(&repo, "README.md");

    yp(&repo)
        .args(["--print", "--no-copy", "--from", "dirname", "README.md"])
        .assert()
        .success()
        .stdout("projects/example-repo/README.md\n");
}

#[test]
fn from_git_renders_repo_relative_path() {
    // Initialise a real git repo so the walk-up detector finds `.git/`.
    let root = tempdir().unwrap();
    let repo = canonical(&root);
    let status = StdCommand::new("git")
        .arg("init")
        .arg("--quiet")
        .current_dir(&repo)
        .status()
        .expect("git must be available for this test");
    assert!(status.success(), "git init failed: {status:?}");

    let src = repo.join("src");
    fs::create_dir(&src).unwrap();
    touch(&src, "lib.rs");

    // From the subdirectory `src/`, asking for `lib.rs` should render
    // `src/lib.rs` relative to the repo root.
    yp(&src)
        .args(["--print", "--no-copy", "--from", "git", "lib.rs"])
        .assert()
        .success()
        .stdout("src/lib.rs\n");
}

#[test]
fn from_git_outside_repo_exits_with_not_in_repo_code() {
    // No `.git` ancestor anywhere → exit code 3 (`YankError::NotInRepo`).
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    touch(&cwd, "a.txt");

    yp(&cwd)
        .args(["--print", "--no-copy", "--from", "git", "a.txt"])
        .assert()
        .failure()
        .code(3);
}

// ---------------------------------------------------------------------------
// 3. --absolute anchor
// ---------------------------------------------------------------------------

#[test]
fn absolute_anchor_renders_canonical_absolute_path() {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    touch(&cwd, "a.txt");

    let expected = format!("{}\n", cwd.join("a.txt").display());

    yp(&cwd)
        .args(["--print", "--no-copy", "--absolute", "a.txt"])
        .assert()
        .success()
        .stdout(expected);
}

// ---------------------------------------------------------------------------
// 4. --relative-to <PATH>
// ---------------------------------------------------------------------------

#[test]
fn relative_to_renders_path_relative_to_given_base() {
    // Layout: <tmp>/projects/example-repo/README.md
    // Base:   <tmp>/projects   → expected: "example-repo/README.md"
    let root = tempdir().unwrap();
    let projects = canonical(&root).join("projects");
    let repo = projects.join("example-repo");
    fs::create_dir_all(&repo).unwrap();
    touch(&repo, "README.md");

    yp(&repo)
        .args(["--print", "--no-copy", "--relative-to"])
        .arg(&projects)
        .arg("README.md")
        .assert()
        .success()
        .stdout("example-repo/README.md\n");
}

// ---------------------------------------------------------------------------
// 5. Multiple operands → newline-joined, order preserved
// ---------------------------------------------------------------------------

#[test]
fn multiple_operands_preserve_order_with_newline_separator() {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    touch(&cwd, "a.txt");
    touch(&cwd, "b.txt");
    touch(&cwd, "c.txt");

    // Deliberately not alphabetical to prove order is preserved.
    yp(&cwd)
        .args([
            "--print",
            "--no-copy",
            "--from",
            "base",
            "c.txt",
            "a.txt",
            "b.txt",
        ])
        .assert()
        .success()
        // Trailing `\n` comes from `StdoutSink::write_line`.
        .stdout(format!(
            "{name}/c.txt\n{name}/a.txt\n{name}/b.txt\n",
            name = cwd.file_name().unwrap().to_string_lossy()
        ));
}

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

// ---------------------------------------------------------------------------
// 9. Missing path → strict failure (NotFound = code 2)
// ---------------------------------------------------------------------------

#[test]
fn missing_operand_causes_strict_non_zero_exit() {
    // Even though `a.txt` exists, the presence of any missing operand must
    // abort the whole run before anything is rendered or copied.
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    touch(&cwd, "a.txt");

    yp(&cwd)
        .args([
            "--print",
            "--no-copy",
            "--from",
            "base",
            "a.txt",
            "missing.txt",
        ])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty()) // nothing emitted before validation
        .stderr(predicate::str::contains("missing.txt"));
}

// ---------------------------------------------------------------------------
// 10. `--print` flag → writes to stdout
// ---------------------------------------------------------------------------

#[test]
fn print_flag_writes_rendered_path_to_stdout() {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    touch(&cwd, "a.txt");

    yp(&cwd)
        .args(["--print", "--no-copy", "--absolute", "a.txt"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a.txt"))
        .stdout(predicate::str::ends_with("\n"));
}

// ---------------------------------------------------------------------------
// 11. `--no-copy` alone → silent success (no stdout, no clipboard)
// ---------------------------------------------------------------------------

#[test]
fn no_copy_without_print_is_silent_and_exits_zero() {
    // We can't observe the clipboard from a test, but we *can* observe
    // that `--no-copy` (without `--print`) emits nothing to stdout and
    // still exits successfully — which proves the clipboard branch was
    // skipped (otherwise headless CI would have written via the fallback
    // sink).
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    touch(&cwd, "a.txt");

    yp(&cwd)
        .args(["--no-copy", "--absolute", "a.txt"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}

// ---------------------------------------------------------------------------
// VCS Integration Tests — Network-free, deterministic
// ---------------------------------------------------------------------------
//
// These tests exercise `--vcs` URL generation using fake `.git/` directories.
// No real git binary or network access is required (test 8 explicitly removes
// git from PATH to verify graceful degradation).

/// Stable 40-hex SHA used across all VCS tests.
const SHA: &str = "abc1234567890123456789012345678901234567";

/// Create a fake `.git/` inside `root` so the walk-up detector finds the repo
/// and `GitDirVcsInfoProvider` can parse it — entirely offline, no `git`.
///
/// * `remote_url` becomes `[remote "origin"] url = <remote_url>`.
/// * `head` is written verbatim to `.git/HEAD` (e.g. `ref: refs/heads/main\n`).
/// * each `(ref_path, sha)` writes `.git/<ref_path>` with `<sha>\n`.
fn fake_git_repo(root: &Path, remote_url: &str, head: &str, refs: &[(&str, &str)]) {
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
// Shell Completions Integration Tests
// ---------------------------------------------------------------------------
//
// These tests exercise the `--completions` flag which generates shell
// completion scripts and exits. Network-free, deterministic.

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
