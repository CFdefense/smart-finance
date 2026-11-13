# Dockerfile to build the Rust backend (the `smart-finance` API server).
#
# Build context = the REPO ROOT (see deploy/docker-compose.yml), because the
# build needs Cargo.toml, Cargo.lock and src/ from there. The repo-root
# .dockerignore keeps everything else (target/, .env, frontend/, ...) out.
#
# This is a "multi-stage" build: stage 1 has the whole Rust toolchain and
# compiles the program; stage 2 is a tiny image that only receives the finished
# binary. The compiler (~1.5 GB) never ends up in the image we actually run.

# ---------- Stage 1: builder ----------
# Full official Rust image, pinned to an exact version so every build uses the
# same compiler. "trixie" = Debian 13. The full (non-slim) image already has a
# C toolchain, pkg-config and the OpenSSL headers (libssl-dev), which the
# openssl-sys crate (pulled in by reqwest's native-tls) needs to compile.
FROM rust:1.99.1-trixie AS builder

# Same path as the runtime stage ON PURPOSE: src/global.rs bakes
# CARGO_MANIFEST_DIR (= this directory) into the binary as the log directory,
# so the program will write its logs to /app/logs wherever it runs.
WORKDIR /app

# Copy only what cargo needs to compile.
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build arguments exist only while building. Declaring them here, inside the
# builder stage, keeps them out of the final image's history.
#
# DATABASE_URL: sqlx's query! macros connect to a live, already-migrated
#   Postgres WHILE COMPILING to type-check every SQL query. No database = no
#   build. (This is why deploy.sh starts and migrates the db first.)
# DOMAIN: read at compile time (option_env!) and used as the login cookie's
#   Domain. It must equal the host people type in the browser (e.g. the VM's
#   IP), or the browser throws the login cookie away.
#
# APP_ENV is deliberately NOT set: "production" would mark cookies Secure,
# and browsers refuse Secure cookies over plain http://.
ARG DATABASE_URL
ARG DOMAIN=localhost

# Compile only the server binary, optimised (--release); --locked makes cargo
# use exactly the versions in Cargo.lock.
# The two --mount=type=cache lines keep downloaded crates and compiled
# dependencies between builds, so a rebuild after a small code change takes
# about a minute instead of ten. Cache mounts are NOT part of the image, so
# the binary is copied out of target/ in the same step.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked --bin smart-finance \
    && cp target/release/smart-finance /usr/local/bin/smart-finance

# ---------- Stage 2: runtime ----------
# Minimal Debian, SAME release (trixie) as the builder. The binary is linked
# against the builder's glibc and OpenSSL; a different release would fail at
# startup with "GLIBC_x.yz not found" style errors.
FROM debian:trixie-slim

# Runtime libraries the binary needs:
#   libssl3t64      - OpenSSL 3 shared library (this is what Debian 13 calls
#                     the package that was "libssl3" on Debian 12)
#   ca-certificates - root certificates, so HTTPS calls (Plaid) can be verified
# Removing the apt lists afterwards keeps the layer small.
RUN apt-get update \
    && apt-get install -y --no-install-recommends libssl3t64 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# The log directory compiled into the binary (see WORKDIR note above).
RUN mkdir -p /app/logs

# Take ONLY the compiled binary from the builder stage.
COPY --from=builder /usr/local/bin/smart-finance /usr/local/bin/smart-finance

# Documentation only: the server listens on 3001 (BIND_ADDRESS is set in
# docker-compose.yml). Publishing the port happens in the compose file.
EXPOSE 3001

CMD ["smart-finance"]
