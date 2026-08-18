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

// ---------------------------------------------------------------------------
// Short-alias exposure in generated completions.
//
// TDD exception: the short aliases already exist in `src/cli.rs`; clap_complete
// derives scripts from `Cli::command()`, so these verification tests pass on
// first run without a fabricated RED failure.
// ---------------------------------------------------------------------------

/// Approved short/long alias mapping under verification.
const ALIAS_PAIRS: &[(&str, &str)] = &[
    ("-f", "--from"),
    ("-r", "--relative-to"),
    ("-a", "--absolute"),
    ("-g", "--glob"),
    ("-p", "--print"),
    ("-n", "--no-copy"),
    ("-v", "--vcs"),
    ("-R", "--vcs-remote"),
    ("-d", "--vcs-default-branch"),
    ("-b", "--vcs-branch-fallback"),
    ("-x", "--vcs-verify"),
    ("-c", "--completions"),
];

/// Run `--completions <shell>` and return the generated script.
fn generate(shell: &str) -> String {
    let dir = tempdir().unwrap();
    let cwd = canonical(&dir);
    let out = yp(&cwd)
        .args(["--completions", shell])
        .output()
        .expect("spawn yank-path");
    assert!(
        out.status.success(),
        "--completions {shell} exited non-zero"
    );
    String::from_utf8(out.stdout).expect("completion script is valid UTF-8")
}

#[test]
fn completions_bash_exposes_short_aliases() {
    let script = generate("bash");
    let opts: std::collections::BTreeSet<&str> = script
        .lines()
        .find(|l| l.contains("opts=\"-"))
        .and_then(|l| {
            l.split_once('"')
                .map(|(_, rest)| rest.trim_end_matches('"'))
        })
        .expect("bash script contains an opts=\"…\" line")
        .split_whitespace()
        .collect();

    for (short, long) in ALIAS_PAIRS {
        assert!(
            opts.contains(short),
            "bash opts missing short flag `{short}`"
        );
        assert!(opts.contains(long), "bash opts missing long flag `{long}`");
    }
    assert!(
        opts.contains("--VCS"),
        "bash opts dropped visible `--VCS` alias"
    );
}

#[test]
fn completions_zsh_exposes_short_aliases() {
    let script = generate("zsh");

    // clap_complete zsh specifiers: value flags use `-x+[` (repeatable `*-x+[`),
    // booleans use `-x[`; long forms use `--long=[` / `--long[`.
    let specs: &[(&str, &str)] = &[
        ("'-f+[", "'--from=["),
        ("'-r+[", "'--relative-to=["),
        ("'-a[", "'--absolute["),
        ("'*-g+[", "'*--glob=["),
        ("'-p[", "'--print["),
        ("'-n[", "'--no-copy["),
        ("'-v[", "'--vcs["),
        ("'-R+[", "'--vcs-remote=["),
        ("'-d+[", "'--vcs-default-branch=["),
        ("'-b[", "'--vcs-branch-fallback["),
        ("'-x[", "'--vcs-verify["),
        ("'-c+[", "'--completions=["),
    ];

    for (short, long) in specs {
        assert!(
            script.contains(short),
            "zsh script missing short specifier `{short}…`"
        );
        assert!(
            script.contains(long),
            "zsh script missing long specifier `{long}…`"
        );
    }
    assert!(
        script.contains("'--VCS["),
        "zsh script dropped visible `--VCS` alias"
    );
}
