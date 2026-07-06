# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Core CLI**: render path strings under anchor modes `--from home|base|parent|git`,
  `--absolute`, and `--relative-to <PATH>`. Anchors are mutually exclusive.
- **VCS URL anchor** (`--vcs`, alias `--VCS`): offline rendering of GitHub, GitLab,
  Bitbucket, and generic host permalinks. Parses `.git/config`, `.git/HEAD`, and
  ref files directly — no `git` subprocess required. Related flags:
  - `--vcs-remote <REMOTE>` (default: `origin`)
  - `--vcs-default-branch <BRANCH>` (default: `main`)
  - `--vcs-branch-fallback` for using branch names when no SHA is resolvable
  - `--vcs-verify` for opt-in remote verification via `git ls-remote`
- Remote URL normalization: SSH (`git@host:owner/repo.git`), `ssh://`, and HTTPS
  forms all resolve to the same host/owner/repo. Nested GitLab groups handled.
- Offline safety warnings (detached HEAD, no upstream, unpushed commits) printed
  to stderr; URL still emitted to stdout with exit code 0.
- **Clipboard support** via `arboard` with `--print` / `--no-copy` flags.
- Automatic headless stdout fallback when no clipboard backend is available.
- **Single-level glob expansion** (`--glob PATTERN`, repeatable): `**` and `/`
  rejected, deterministic sort, no dedup, no-match is fatal.
- **Strict all-or-nothing validation**: every operand must exist; partial results
  never touch the clipboard.
- **Shell completions** (`--completions <SHELL>`) for bash, zsh, fish, elvish,
  and powershell via `clap_complete`.
- Distinct exit codes per error category for scripting reliability.
- Dual-license files: `LICENSE-MIT` and `LICENSE-APACHE`.

### Changed

- MSRV set to Rust 1.88 (required by the transitive `image` crate via `arboard`; clap/clap_complete need 1.85 and Cargo.lock is v4).
- CI matrix covers Linux and macOS with MSRV verification (Windows is future work).
- Security CI jobs: `cargo audit` and `cargo deny`.
- Prebuilt release binaries via GitHub Actions.
- Criterion benchmarks for performance regression testing.
- Integration tests split by concern for maintainability.
- Mold linker and mise toolchain for faster local development.
- Multi-stage Docker image with git/openssh for `--vcs-verify` support.

### Fixed

- `ConflictingAnchors` error message now includes `--vcs` in the conflict list.
- Docker image build: added stub bench file in dependency-cache layer so `[[bench]]`
  target compiles; real `benches/` copied before final build.
- Corrected declared MSRV to 1.88 to match actual dependency floor (`image@0.25.10` via `arboard`).
- Docker runtime base aligned to `debian:trixie-slim` to match builder's glibc.

[Unreleased]: https://github.com/neumachen/yank-path/compare/main...HEAD
