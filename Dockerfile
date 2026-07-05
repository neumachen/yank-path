# syntax=docker/dockerfile:1.7

# -----------------------------------------------------------------------------
# Stage 1: builder
# -----------------------------------------------------------------------------
# Build the release binary on a slim Rust toolchain image. Dependencies are
# cached by copying Cargo.toml/Cargo.lock first against a dummy main.rs, then
# the real sources are copied in and rebuilt. This keeps `docker build` fast
# on incremental source-only changes.
FROM rust:1.90-slim AS builder

# Minimal build essentials. `arboard` on Linux uses the pure-Rust `x11rb`
# crate (no libxcb headers required), so we only need a C toolchain for
# crates that ship build.rs with C shims and pkg-config for safety.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# ---- Dependency caching layer --------------------------------------------
# Copy only the manifests, synthesize a dummy binary + library + bench so cargo
# can resolve and compile every third-party dependency. This layer is reused as
# long as Cargo.toml / Cargo.lock are unchanged.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src benches \
    && echo 'fn main() {}' > src/main.rs \
    && echo '// dummy' > src/lib.rs \
    && echo 'fn main() {}' > benches/parsers.rs \
    && cargo build --release \
    && rm -rf src benches \
        target/release/yank-path* \
        target/release/libyank_path* \
        target/release/deps/yank_path* \
        target/release/deps/libyank_path* \
        target/release/.fingerprint/yank-path-*

# ---- Real build ----------------------------------------------------------
COPY src ./src
COPY benches ./benches
RUN cargo build --release \
    && strip target/release/yank-path || true

# -----------------------------------------------------------------------------
# Stage 2: runtime
# -----------------------------------------------------------------------------
# Slim Debian base for glibc compatibility with the dynamically linked
# release binary. Must match the builder's Debian version (rust:1.90-slim uses
# trixie with glibc 2.39). Note: the `arboard` clipboard backend requires a
# display server (X11/Wayland) which is unavailable inside this container, so
# the CLI will gracefully fall back to writing the rendered path(s) to stdout.
FROM debian:trixie-slim AS runtime

# ca-certificates: HTTPS trust roots.
# git + openssh-client: only needed for the opt-in `--vcs-verify` flag, which
# runs `git ls-remote` over HTTPS/SSH. All other `--vcs` URL rendering is
# fully offline and needs neither. Kept minimal via --no-install-recommends.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        git \
        openssh-client \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/yank-path /usr/local/bin/yank-path

# Clipboard backend (arboard) needs X11/Wayland; in a container there is no
# display server, so yank-path falls back to printing rendered paths on stdout.
ENTRYPOINT ["/usr/local/bin/yank-path"]
