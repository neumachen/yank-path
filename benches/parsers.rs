//! Criterion benchmarks for VCS hot-path parsers.
//!
//! These benchmarks exercise `GitDirVcsInfoProvider::info()` — the end-to-end
//! git config + HEAD + ref resolution path — using deterministic tempfile
//! fixtures. No network, no real `git` subprocess.

use std::fs;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::{tempdir, TempDir};
use yank_path::{GitDirVcsInfoProvider, VcsInfoProvider};

/// Stable 40-hex SHA for benchmarks.
const SHA: &str = "abc1234567890123456789012345678901234567";

/// Create a fake `.git/` directory with the given remote URL and refs.
fn setup_fake_git_repo(remote_url: &str, head: &str, refs: &[(&str, &str)]) -> TempDir {
    let tmp = tempdir().unwrap();
    let git_dir = tmp.path().join(".git");
    fs::create_dir(&git_dir).unwrap();

    let config = format!("[remote \"origin\"]\n\turl = {remote_url}\n");
    fs::write(git_dir.join("config"), config).unwrap();
    fs::write(git_dir.join("HEAD"), head).unwrap();

    for (ref_path, sha) in refs {
        let full_path = git_dir.join(ref_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full_path, format!("{sha}\n")).unwrap();
    }

    tmp
}

/// Benchmark: GitHub SSH remote, single branch ref.
fn bench_github_ssh_repo(c: &mut Criterion) {
    let tmp = setup_fake_git_repo(
        "git@github.com:user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    let provider = GitDirVcsInfoProvider::new();

    c.bench_function("GitDirVcsInfoProvider::info (github ssh)", |b| {
        b.iter(|| {
            let info = provider.info(black_box(tmp.path()), "origin").unwrap();
            black_box(info)
        });
    });
}

/// Benchmark: HTTPS remote URL.
fn bench_https_repo(c: &mut Criterion) {
    let tmp = setup_fake_git_repo(
        "https://github.com/user/repo.git",
        "ref: refs/heads/main\n",
        &[("refs/heads/main", SHA)],
    );
    let provider = GitDirVcsInfoProvider::new();

    c.bench_function("GitDirVcsInfoProvider::info (https)", |b| {
        b.iter(|| {
            let info = provider.info(black_box(tmp.path()), "origin").unwrap();
            black_box(info)
        });
    });
}

/// Benchmark: GitLab nested group (org/group/subgroup/repo).
fn bench_gitlab_nested_group(c: &mut Criterion) {
    let tmp = setup_fake_git_repo(
        "git@gitlab.com:org/group/subgroup/repo.git",
        "ref: refs/heads/develop\n",
        &[("refs/heads/develop", SHA)],
    );
    let provider = GitDirVcsInfoProvider::new();

    c.bench_function("GitDirVcsInfoProvider::info (gitlab nested)", |b| {
        b.iter(|| {
            let info = provider.info(black_box(tmp.path()), "origin").unwrap();
            black_box(info)
        });
    });
}

/// Benchmark: Detached HEAD (raw SHA in HEAD file).
fn bench_detached_head(c: &mut Criterion) {
    let tmp = setup_fake_git_repo(
        "git@github.com:user/repo.git",
        &format!("{SHA}\n"),
        &[], // no refs needed for detached
    );
    let provider = GitDirVcsInfoProvider::new();

    c.bench_function("GitDirVcsInfoProvider::info (detached HEAD)", |b| {
        b.iter(|| {
            let info = provider.info(black_box(tmp.path()), "origin").unwrap();
            black_box(info)
        });
    });
}

/// Benchmark: Repo with packed-refs (SHA resolved from packed-refs file).
fn bench_packed_refs(c: &mut Criterion) {
    let tmp = tempdir().unwrap();
    let git_dir = tmp.path().join(".git");
    fs::create_dir(&git_dir).unwrap();

    let config = "[remote \"origin\"]\n\turl = git@github.com:user/repo.git\n";
    fs::write(git_dir.join("config"), config).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

    // Write packed-refs instead of loose ref
    let packed_refs = format!(
        "# pack-refs with: peeled fully-peeled sorted\n\
         {SHA} refs/heads/main\n"
    );
    fs::write(git_dir.join("packed-refs"), packed_refs).unwrap();

    let provider = GitDirVcsInfoProvider::new();

    c.bench_function("GitDirVcsInfoProvider::info (packed-refs)", |b| {
        b.iter(|| {
            let info = provider.info(black_box(tmp.path()), "origin").unwrap();
            black_box(info)
        });
    });
}

/// Benchmark: Config with branch upstream tracking.
fn bench_with_upstream_tracking(c: &mut Criterion) {
    let tmp = tempdir().unwrap();
    let git_dir = tmp.path().join(".git");
    fs::create_dir(&git_dir).unwrap();

    let config = r#"[remote "origin"]
    url = git@github.com:user/repo.git
    fetch = +refs/heads/*:refs/remotes/origin/*
[branch "main"]
    remote = origin
    merge = refs/heads/main
"#;
    fs::write(git_dir.join("config"), config).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();

    // Create both local and remote tracking refs
    fs::create_dir_all(git_dir.join("refs/heads")).unwrap();
    fs::create_dir_all(git_dir.join("refs/remotes/origin")).unwrap();
    fs::write(git_dir.join("refs/heads/main"), format!("{SHA}\n")).unwrap();
    fs::write(git_dir.join("refs/remotes/origin/main"), format!("{SHA}\n")).unwrap();

    let provider = GitDirVcsInfoProvider::new();

    c.bench_function("GitDirVcsInfoProvider::info (with upstream)", |b| {
        b.iter(|| {
            let info = provider.info(black_box(tmp.path()), "origin").unwrap();
            black_box(info)
        });
    });
}

criterion_group!(
    benches,
    bench_github_ssh_repo,
    bench_https_repo,
    bench_gitlab_nested_group,
    bench_detached_head,
    bench_packed_refs,
    bench_with_upstream_tracking,
);

criterion_main!(benches);
