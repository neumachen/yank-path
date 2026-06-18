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
