# yank-path

[![CI](https://github.com/neumachen/yank-path/actions/workflows/ci.yml/badge.svg)](https://github.com/neumachen/yank-path/actions/workflows/ci.yml)
[![Docker](https://github.com/neumachen/yank-path/actions/workflows/docker.yml/badge.svg)](https://github.com/neumachen/yank-path/actions/workflows/docker.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Crates.io](https://img.shields.io/crates/v/yank-path.svg)](https://crates.io/crates/yank-path)

`yank-path` is a small, composable Rust CLI for rendering the **textual
representation** of one or more filesystem paths under a chosen anchor
form and copying the result to the system clipboard. It exists because
developers most often need the path *string* — to paste into chat
messages, issues, pull requests, docs, configuration files, log lines —
not the file itself. `yank-path` makes producing that string a single,
predictable command instead of an ad-hoc mixture of `pwd`, `realpath`,
`basename`, `dirname`, and shell variable substitutions.

## Synopsis

```
yank-path [OPTIONS] [PATH...]
```

- With no operand, `yank-path` defaults to `.` (the current directory).
- Multiple operands are rendered **one per line**, in the order given.
- All options apply to **every** operand in the invocation.

## Installation

### From crates.io (recommended)

```sh
cargo install yank-path
```

### From source (latest `main`)

Install the latest unreleased revision directly from GitHub:

```sh
cargo install --git https://github.com/neumachen/yank-path
```

Or clone and build manually:

```sh
git clone https://github.com/neumachen/yank-path
cd yank-path
cargo build --release
# binary at target/release/yank-path
```

### Runtime notes

The binary is self-contained — once built, it has no runtime
dependencies beyond a system clipboard backend, and even that is
optional. On **headless systems** with no X11 or Wayland display (CI
runners, SSH sessions, the Docker image shipped with this repo)
`yank-path` automatically falls back to printing the rendered text to
stdout instead of copying it to the clipboard, so the command remains
useful everywhere.

If you would rather not install a toolchain locally, the provided
Docker image works as a drop-in alternative — see
[Container usage](#container-usage) for the full workflow:

```sh
docker build -t yank-path .
docker run --rm yank-path .
```

### Requirements

Building or installing from source requires a stable Rust toolchain
(`cargo` 1.70 or newer is known to work; any reasonably current stable
release should be fine). No other system packages are required on
Linux, macOS, or Windows.

## Anchor modes

The anchor controls *which form* of the path string is produced.
`--from`, `--absolute`, `--relative-to`, and `--vcs` are **mutually
exclusive**; supplying more than one is a usage error.

Examples below assume the current working directory is
`~/projects/example-repo` (i.e. `/Users/example/projects/example-repo`)
and the command is `yank-path .`.

| Anchor          | Aliases  | Flag                     | Output                                                                |
| --------------- | -------- | ------------------------ | --------------------------------------------------------------------- |
| home (default)  | —        | `--from home`            | `~/projects/example-repo` (falls back to absolute if outside `$HOME`) |
| base            | basename | `--from base`            | `example-repo`                                                        |
| parent          | dirname  | `--from parent`          | `projects/example-repo`                                               |
| git             | —        | `--from git`             | `.` (relative to repo root; errors if not in a repo)                  |
| absolute        | —        | `--absolute`             | `/Users/example/projects/example-repo`                                |
| relative-to     | —        | `--relative-to <PATH>`   | e.g. `--relative-to ~/projects` → `example-repo`                      |
| vcs             | VCS      | `--vcs`                  | VCS remote URL permalink — see [VCS URL anchor](#vcs-url-anchor)      |

Notes:

- **No anchor ever emits a bare filename.** Every rendered output
  contains at least one containing-directory segment, so the result is
  unambiguous when pasted into prose.
- **Git detection walks up the filesystem** looking for a `.git` entry.
  No `git` subprocess is invoked. `--from git` exits non-zero when no
  ancestor `.git` is found.
- **`home` falls back to absolute** when the target is outside `$HOME`,
  rather than producing a confusing `~/../...` form.

## VCS URL anchor

`--vcs` renders each path as a **VCS remote URL** instead of a local
path string. This is useful for sharing stable permalinks to files in
pull requests, issues, or documentation.

By default the feature is **fully offline** — it parses `.git/config`,
`.git/HEAD`, and ref files directly to resolve the remote URL, current
commit SHA, and branch. No `git` subprocess is invoked unless you
explicitly request `--vcs-verify`.

### Flags

| Flag                          | Default  | Description                                                        |
| ----------------------------- | -------- | ------------------------------------------------------------------ |
| `--vcs` (alias `--VCS`)       | —        | Enable VCS URL mode (mutually exclusive with other anchor modes)   |
| `--vcs-remote <REMOTE>`       | `origin` | Which git remote to use                                            |
| `--vcs-default-branch <BRANCH>` | `main` | Branch to use when falling back                                    |
| `--vcs-branch-fallback`       | off      | Use a branch name instead of a SHA when no commit is resolvable    |
| `--vcs-verify`                | off      | Opt-in check that the ref exists on the remote via `git ls-remote` |

### Commit SHA vs branch

`--vcs` prefers the **commit SHA** to produce a stable permalink. If no
SHA is resolvable (e.g. a freshly initialized repo with no commits) and
`--vcs-branch-fallback` is **not** set, the command errors with exit
code 12 and a message suggesting `--vcs-branch-fallback`.

### Host-aware URL formats

Remote URLs are normalized from SSH, `ssh://`, or HTTPS forms and
rendered with the appropriate URL structure for each host:

| Host            | URL format                                              |
| --------------- | ------------------------------------------------------- |
| github.com      | `https://github.com/{owner}/{repo}/blob/{ref}/{path}`   |
| gitlab.com      | `https://gitlab.com/{owner}/{repo}/-/blob/{ref}/{path}` |
| bitbucket.org   | `https://bitbucket.org/{owner}/{repo}/src/{ref}/{path}` |
| other (generic) | `https://{host}/{owner}/{repo}/blob/{ref}/{path}`       |

Nested GitLab groups (e.g. `org/group/subgroup/repo`) are handled
correctly — the owner portion preserves the full group path.

### Offline safety warnings

When local state looks risky — detached HEAD, no upstream configured, or
local commits not pushed — a warning is printed to **stderr**:

```
yank-path: warning: commit abc1234 may not exist on remote 'origin' (no upstream, local-only commits)
```

The URL is still produced on **stdout** and the exit code remains 0.
This lets you pipe stdout cleanly while seeing warnings in the terminal.

### Remote verification (`--vcs-verify`)

`--vcs-verify` spawns a `git ls-remote` subprocess (with a timeout
guard) to confirm the ref exists on the remote:
- **Ref found:** no extra output.
- **Ref not found:** warning printed to stderr.
- **Verification failed** (git missing, unreachable, timeout): neutral
  note printed to stderr.

In all cases the URL is still produced and exit code is unchanged (0).
`--vcs-verify` requires `git` on PATH; without it you receive a neutral
note.

### Examples

```sh
# GitHub permalink to a file at the current commit
yank-path --vcs --print --no-copy src/main.rs
# -> https://github.com/owner/repo/blob/abc1234.../src/main.rs

# Use a branch instead of a SHA when no commit is resolvable
yank-path --vcs --vcs-branch-fallback --print --no-copy src/main.rs

# Point at a non-default remote
yank-path --vcs --vcs-remote upstream --print --no-copy README.md

# Opt-in: verify the ref exists on the remote (requires git on PATH)
yank-path --vcs --vcs-verify --print --no-copy src/main.rs
```

## Glob expansion

`--glob PATTERN` expands a single-level glob in the current working
directory. The flag is **repeatable** and the results are concatenated.

Rules:

- **Single-level only.** A pattern may not contain `/`; multi-segment
  patterns are rejected.
- **`**` is rejected** with a dedicated error. Recursive globbing is
  explicitly out of scope for v1.
- **Files only.** Directories that happen to match the pattern are
  silently skipped.
- **Sorted deterministically.** Aggregate results are sorted
  lexicographically across all patterns so output is stable across
  runs.
- **No deduplication.** If two patterns both match the same file, that
  file is rendered twice.
- **Evaluated relative to the current working directory.**
- **No-match is fatal.** If the union of all `--glob` patterns matches
  zero files, `yank-path` exits non-zero and **does not modify the
  clipboard**.

Bare shell-expanded globs (e.g. `yank-path *.md`) still work — those
become ordinary positional operands and are processed by the strict
validation path described below, not the `--glob` machinery.

## Clipboard and output

- **Default:** rendered text is copied to the system clipboard via
  [`arboard`](https://docs.rs/arboard). Nothing is written to stdout.
- **`--print`:** also write the rendered text to stdout. The clipboard
  is still updated unless `--no-copy` is set.
- **`--no-copy`:** suppress the clipboard write entirely. Combined with
  `--print` this becomes a pure stdout renderer. Used alone it is a
  silent success (useful as a validation step in scripts).
- **Headless fallback:** on hosts where no clipboard backend is
  available (e.g. a Linux container with no X/Wayland session),
  `yank-path` automatically writes the rendered text to stdout instead
  of failing. Combining the fallback with `--print` does not
  double-emit.

## Shell completions

`yank-path --completions <SHELL>` prints a completion script to stdout
for the given shell; redirect it to the appropriate location for your
shell. This is a standalone action — it ignores all other arguments,
does not touch the clipboard or filesystem, and exits 0.

Supported shells: `bash`, `zsh`, `fish`, `elvish`, `powershell`.

### Bash (user-local)

```sh
mkdir -p ~/.local/share/bash-completion/completions
yank-path --completions bash > ~/.local/share/bash-completion/completions/yank-path
```

### Zsh

```sh
mkdir -p ~/.zsh/completions
yank-path --completions zsh > ~/.zsh/completions/_yank-path
# ensure ~/.zshrc has (before `compinit`):
#   fpath=(~/.zsh/completions $fpath)
```

### Fish

```sh
mkdir -p ~/.config/fish/completions
yank-path --completions fish > ~/.config/fish/completions/yank-path.fish
```

Elvish and PowerShell are also supported — use `--completions elvish`
or `--completions powershell` and redirect to the appropriate location
for those shells.

## Strict validation

Validation is **all-or-nothing**. Every positional operand must exist
on disk; if any operand fails verification, `yank-path`:

1. exits with a non-zero status,
2. prints the offending path to stderr, and
3. **does not modify the clipboard**.

This means a successful `yank-path` invocation always corresponds to a
clipboard that contains the full, valid result — there is no partial
state to reason about.

## Examples

```sh
# Default: yank `~/projects/example-repo` (home-anchored) for the cwd.
yank-path

# Repo-relative path for a source file (errors if not in a Git repo).
yank-path --from git src/main.rs

# Canonical absolute path of a file.
yank-path --absolute README.md

# Print all *.rs files in the cwd to stdout without touching the clipboard.
yank-path --glob '*.rs' --print --no-copy

# Render a path relative to an explicit base directory.
yank-path --relative-to ~ ~/projects/example-repo/README.md
# → projects/example-repo/README.md
```

## Container usage

`yank-path` ships with a multi-stage `Dockerfile`, a `docker-compose.yml`
for development, and a `Makefile` that wraps the common workflows.

Makefile targets:

| Target          | Action                                                 |
| --------------- | ------------------------------------------------------ |
| `build`         | `cargo build --release`                                |
| `test`          | `cargo test`                                           |
| `lint`          | `cargo clippy --all-targets -- -D warnings`            |
| `fmt`           | `cargo fmt`                                            |
| `fmt-check`     | `cargo fmt --check`                                    |
| `docker-build`  | `docker build -t yank-path:latest .`                   |
| `docker-test`   | `docker compose run --rm dev cargo test`               |
| `clean`         | `cargo clean`                                          |

The `dev` compose service builds the Dockerfile's `builder` stage
(which carries the full Rust toolchain), mounts the working tree at
`/app`, and caches `cargo` registry, git, and `target/` directories in
named volumes for fast incremental rebuilds.

```sh
docker compose run --rm dev               # runs `cargo test`
docker compose run --rm dev cargo clippy  # any cargo command
docker compose run --rm dev bash          # interactive shell
```

Inside a container there is **no clipboard backend**, so any
`yank-path` invocation automatically exercises the stdout-fallback
path described above. This is a useful smoke test for CI as well:

```sh
docker run --rm yank-path:latest --absolute /data
# /data
```

## Deferred (out of scope for v1)

The following are intentionally **not** implemented. They are listed
here so future contributors and users know what to expect, and to make
clear that the current surface is minimal by design.

- `--permissive` mode (partial-success semantics).
- Deduplication / `--unique` flag.
- `--stdin` input (reading operands from standard input).
- Fuzzy path matching.
- Regular-expression matching.
- Recursive globbing — `**` is **rejected** with a dedicated error,
  not silently treated as a wildcard.
- NUL-separated I/O (`-0` style separators).
- Configuration files or environment-variable defaults.
- Man pages.
- OS-level packaging (Homebrew, deb/rpm, etc.).

## License

Dual-licensed under MIT or Apache-2.0, at your option.
