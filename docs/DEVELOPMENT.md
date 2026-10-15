# Development Guide

## Prerequisites

- [Rust](https://rustup.rs) 1.80+
- [Docker](https://docs.docker.com/get-docker/) & Docker Compose
- [sqlx-cli](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli): `cargo install sqlx-cli --no-default-features --features postgres`
- [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) (for coverage): `cargo install cargo-tarpaulin`

---

## Setup

```bash
# 1. Start Postgres
docker compose up -d db

# 2. Configure environment
cp .env.example .env  # defaults work out of the box

# 3. Run migrations
sqlx migrate run

# 4. Start the server (debug mode — Swagger UI available at /swagger)
cargo run
```

---

## Running Tests

```bash
cargo test
```

Integration tests require Postgres to be running (`docker compose up -d db`).

---

## Local CI Pipeline

`cicd.sh` mirrors `.github/workflows/ci.yml` exactly. It auto-installs `sqlx-cli` and `cargo-tarpaulin` if they are missing.

```bash
./cicd.sh              # full pipeline (migrate, fmt, clippy, build, coverage, openapi)
./cicd.sh fmt          # formatting check only
./cicd.sh clippy       # clippy only
./cicd.sh test         # tests + coverage only
./cicd.sh openapi      # regenerate docs/openapi.json
```

Coverage reports are written to `docs/coverage/` (gitignored).

---

## Coverage Configuration

Coverage thresholds and file exclusions live in the tarpaulin config files — edit these to adjust:

| File | Profile | Threshold |
|---|---|---|
| [`tarpaulin.toml`](../tarpaulin.toml) | Unit tests (`--run-types Tests`) | ≥ 85% |
| [`tarpaulin-integration.toml`](../tarpaulin-integration.toml) | Integration tests (`--run-types Bins`) | ≥ 75% |

---

## Environment Variables

Copy `.env.example` to `.env`. The defaults match the Docker Compose Postgres service.

| Variable | Description |
|---|---|
| `DATABASE_URL` | Postgres connection string |
| `FRONTEND_URL` | Allowed CORS origin |
| `BIND_ADDRESS` | Server bind address (e.g. `127.0.0.1:3001`) |
| `RUST_LOG` | Log level (e.g. `debug`, `info`) |
