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

# 4. Seed Plaid Sandbox dummy data for testers (optional)
cargo run --bin seed-sandbox

# 5. Start the server (debug mode — Swagger UI available at /swagger)
cargo run
```

---

## Connecting to live PSQL Database
```bash
psql "postgres://postgres:postgres@localhost:5432/smart_finance"
```

---

## Seeding Plaid Sandbox Data

To populate PostgreSQL with pre-configured tester accounts, linked institutions, financial accounts, and live Plaid Sandbox transactions:

```bash
# 1. Ensure .env has valid PLAID_CLIENT_ID and PLAID_SECRET credentials
# 2. Run the seeder
cargo run --bin seed-sandbox

# To reset tester accounts and re-sync fresh sandbox data:
cargo run --bin seed-sandbox -- --reset
```

The seeder creates 4 tester accounts configured in [`fixtures/sandbox-users.json`](../fixtures/sandbox-users.json) with default password `Password123`:
- `jeremy@test.com` (`custom_jfrank` / `pass_good`)
- `libby@test.com` (`custom_lfoley` / `pass_good`)
- `mitchell@test.com` (`custom_mlevy` / `pass_good`)
- `christian@test.com` (`custom_cfarrell` / `pass_good`)

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
./cicd.sh --reset      # resets the database and then runs the full pipeline
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

## Deploying with Docker

Everything runs in containers defined in [`deploy/docker-compose.yml`](../deploy/docker-compose.yml): `db` (Postgres), `ai-service`, `backend` (Rust API, port 3001) and `frontend` (nginx serving the Angular build, port 80).

### Prerequisites (on the VM)

- Docker with the Compose plugin (`docker compose version`)
- `sqlx-cli` (see [Prerequisites](#prerequisites))
- A `.env` file in the repo root. It is **not** in git; copy `.env.example` and fill it in. On the VM it must set:

  | Variable | Value on the VM | Why |
  |---|---|---|
  | `FRONTEND_URL` | `http://<vm-ip>` | The only origin the backend's CORS policy accepts. Must match the address typed in the browser exactly (no trailing slash). |
  | `API_URL` | `http://<vm-ip>:3001/api` | Baked into the frontend at build time; the browser calls the backend directly. |
  | `DOMAIN` | `<vm-ip>` | Compiled into the backend as the login cookie's domain. If it doesn't match the browser's address, login "succeeds" but the cookie is dropped. |

  `API_URL` and `DOMAIN` are build-time values: after changing them, rebuild (re-run the deploy script).

### Deploy

```bash
bash deploy/deploy.sh
```

The script runs these steps, and the order matters:

1. `git switch main` and `git pull --ff-only origin main`.
2. Start the database and wait until it is healthy (`up -d --wait db`).
3. Run migrations (`sqlx migrate run`).
4. Build the backend image.
5. Build the other images and start everything.

**Why the order matters:** the backend's `sqlx::query!` macros connect to the database *while compiling* to check every SQL query against the real schema. The backend image therefore cannot be built until Postgres is running **and** migrated. The build reaches Postgres on `127.0.0.1:5432` using host networking (`network: host` in the compose file).

Newer Docker Compose versions refuse host-network builds started from `docker compose build`/`up --build` (`additional privileges requested: pass "--allow=network.host"`). The script handles this by retrying the backend build through `docker buildx bake --allow network.host`. For the same reason, don't use `up --build` for the backend by hand; use the script.

### Check it worked

```bash
docker compose -f deploy/docker-compose.yml --env-file .env ps            # 4 services "Up", db "healthy"
docker compose -f deploy/docker-compose.yml --env-file .env logs backend  # "Server starting on 0.0.0.0:3001"
curl -i http://<vm-ip>:3001/api/user/me                                   # 401 = backend is up and answering
```

Then open `http://<vm-ip>` in a browser and log in. If login fails, open the browser dev tools: a CORS error means `FRONTEND_URL` is wrong; a cookie warning means `DOMAIN` is wrong.

### Building on macOS (Docker Desktop)

`deploy.sh` is written for the Linux VM (it also runs `git switch main`). To try the containers on a Mac, run the same steps by hand from the repo root:

```bash
COMPOSE="docker compose -f deploy/docker-compose.yml --env-file .env"
$COMPOSE up -d --wait db
sqlx migrate run
$COMPOSE build --print backend | docker buildx bake --file - --allow network.host --allow "fs.read=$PWD"
$COMPOSE build ai-service frontend
$COMPOSE up -d
```

Differences from the VM:

- Host networking works differently in Docker Desktop than on Linux. If the backend build cannot reach Postgres on `127.0.0.1:5432`, check that host networking is enabled in Docker Desktop's settings (Resources → Network).
- The frontend is served at `http://localhost` (port 80), so `FRONTEND_URL` in `.env` must be `http://localhost` while testing the containers, not the `ng serve` address (`http://localhost:4200`). The `API_URL` and `DOMAIN` defaults already suit localhost.
- The backend container takes port 3001, so stop it (`$COMPOSE stop backend frontend`) before going back to `cargo run`.
