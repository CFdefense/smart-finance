# smart-finance
An AI-Powered Finance Tracker

## Development

### Prerequisites

- Rust 1.80+
- PostgreSQL 16+
- Docker & Docker Compose (for local CI)

### Setup

```bash
cp .env.example .env
# Edit .env — set DATABASE_URL to your local Postgres instance
sqlx migrate run
cargo run
```

### Running tests locally

```bash
cargo test
```

### Run the full CI pipeline locally (Docker)

Mirrors `.github/workflows/ci.yml` exactly — same Postgres image, same steps.

```bash
docker compose run --rm ci
```

This will:
1. Spin up a Postgres 16 container
2. Run migrations
3. Check formatting (`cargo fmt --check`)
4. Run Clippy (`-D warnings`)
5. Build
6. Run unit tests with 90% coverage gate
7. Run integration tests with 75% coverage gate
8. Generate `docs/openapi.json`

Coverage HTML report is written to `docs/coverage/` on the host.

### API Documentation

Swagger UI is available at `http://localhost:3001/swagger` when running in debug mode.

Published spec: <https://cfdefense.github.io/smart-finance/>
