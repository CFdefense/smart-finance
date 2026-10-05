# smart-finance
An AI-Powered Finance Tracker

## Development

### Prerequisites

- Rust 1.80+
- PostgreSQL 16+
- Docker & Docker Compose (for local CI)

### Setup

```bash
# Start the database
docker compose up -d db

cp .env.example .env  # edit if needed

sqlx migrate run
cargo run
```

### Running tests

```bash
cargo test
```

### Run the full CI pipeline locally

Mirrors `.github/workflows/ci.yml` exactly. Requires `sqlx-cli` and `cargo-tarpaulin` installed on your machine.

```bash
# Start the database first if not already running
docker compose up -d db

./ci-local.sh          # full pipeline
./ci-local.sh fmt      # formatting only
./ci-local.sh clippy   # clippy only
./ci-local.sh test     # tests + coverage only
```

Coverage HTML report is written to `docs/coverage/`.

### API Documentation

Swagger UI is available at `http://localhost:3001/swagger` when running in debug mode.

Published spec: <https://cfdefense.github.io/smart-finance/>
