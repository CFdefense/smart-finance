#!/usr/bin/env bash
# cicd.sh: single source of truth for the CI/CD pipeline.
# Used by .github/workflows/ci.yml and for local pre-push checks.
#
# Database: run `docker compose up -d db` first (local), or set DATABASE_URL (CI).
#
# Usage:
#   ./cicd.sh              # full pipeline, preserves database
#   ./cicd.sh --reset      # reset database, then run full pipeline
#   ./cicd.sh reset-db     # drop, recreate and migrate database only
#   ./cicd.sh fmt          # formatting only
#   ./cicd.sh clippy       # clippy only
#   ./cicd.sh python       # python checks only
#   ./cicd.sh test         # tests + coverage only
#   ./cicd.sh openapi      # generate openapi.json only

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo -e "${GREEN}✓ $1${NC}"; }
fail() { echo -e "${RED}✗ $1${NC}"; exit 1; }
step() { echo -e "\n${YELLOW}▶ $1${NC}"; }
info() { echo -e "${YELLOW}  $1${NC}"; }

# ── Load environment ──────────────────────────────────────────────────────────
# Load .env when running locally; in CI DATABASE_URL is set via environment.
if [ -z "${DATABASE_URL:-}" ] && [ -f .env ]; then
    set -a
    source .env
    set +a
fi

# ── Database helpers ──────────────────────────────────────────────────────────
# Splits DATABASE_URL into DB_NAME and ADMIN_URL, which points to the
# maintenance "postgres" database. A database cannot be dropped while
# connected to it.
db_parts() {
    local base="${DATABASE_URL%%\?*}"
    local query="${DATABASE_URL#"$base"}"

    DB_NAME="${base##*/}"
    ADMIN_URL="${base%/*}/postgres${query}"
}

# Run SQL against the maintenance database.
# Use host psql when available, otherwise use the Docker Compose database.
psql_admin() {
    if command -v psql &>/dev/null &&
        psql "$ADMIN_URL" -v ON_ERROR_STOP=1 -qc "$1" 2>/dev/null; then
        return 0
    fi

    docker compose exec -T db \
        psql -U "${POSTGRES_USER:-postgres}" -d postgres \
        -v ON_ERROR_STOP=1 -qc "$1"
}

# ── Reset database ────────────────────────────────────────────────────────────
# Drops the target database, recreates it, and applies all migrations.
run_reset_db() {
    step "Reset database"

    : "${DATABASE_URL:?DATABASE_URL must be set}"

    db_parts

    case "$DB_NAME" in
        *prod*)
            fail "Refusing to reset database '$DB_NAME'"
            ;;
    esac

    if [ "$DB_NAME" = "postgres" ] || [ -z "$DB_NAME" ]; then
        fail "Refusing to reset maintenance database"
    fi

    ensure_tool sqlx \
        "cargo install sqlx-cli --no-default-features --features postgres --locked"

    # PostgreSQL 13+ supports WITH (FORCE) to terminate active connections.
    psql_admin "DROP DATABASE IF EXISTS \"${DB_NAME}\" WITH (FORCE);"

    sqlx database create --database-url "$DATABASE_URL"
    sqlx migrate run --database-url "$DATABASE_URL"

    pass "Database reset and migrations applied"
}

# ── Ensure required tools are installed ───────────────────────────────────────
ensure_tool() {
    local cmd="$1"
    local install_cmd="$2"

    if ! command -v "$cmd" &>/dev/null; then
        info "$cmd not found; installing..."
        eval "$install_cmd"
    fi
}

ensure_dependencies() {
    ensure_tool sqlx \
        "cargo install sqlx-cli --no-default-features --features postgres --locked"

    ensure_tool cargo-tarpaulin \
        "cargo install cargo-tarpaulin --locked"

    ensure_tool uv \
        'curl -LsSf https://astral.sh/uv/install.sh | sh && export PATH="$HOME/.local/bin:$PATH"'

    if ! command -v uv &>/dev/null && [ -x "$HOME/.local/bin/uv" ]; then
        export PATH="$HOME/.local/bin:$PATH"
    fi

    (cd ./frontend && npm install --silent)
}

# ── Migrations ────────────────────────────────────────────────────────────────
# Normal runs only apply pending migrations. They never reset the database.
run_migrate() {
    step "Migrations"
    sqlx migrate run --database-url "$DATABASE_URL"
    pass "Migrations"
}

# ── Rust checks ───────────────────────────────────────────────────────────────
run_fmt() {
    step "Format check"
    cargo fmt --check
    pass "fmt"
}

run_clippy() {
    step "Clippy"
    cargo clippy -- -D warnings
    pass "clippy"
}

run_build() {
    step "Build"
    cargo build --locked
    pass "build"
}

run_unit_coverage() {
    step "Unit tests (≥85% coverage)"
    cargo tarpaulin --config tarpaulin.toml
    pass "unit coverage"
}

run_integration_coverage() {
    step "Integration tests (≥75% coverage)"
    cargo tarpaulin --config tarpaulin-integration.toml
    pass "integration coverage"
}

run_openapi() {
    step "Generate openapi.json"
    cargo run --bin gen-openapi
    pass "openapi.json"
}

# ── Frontend checks ────────────────────────────────────────────────────────────
run_nglint() {
    step "Angular lint"
    (cd ./frontend && npx ng lint)
}

run_prettier_check() {
    step "Prettier check"
    (
        cd ./frontend
        npx prettier --check "src/**/*.{ts,css,html}"
    )
    pass ".prettierrc"
}

run_angular_build() {
    step "Angular build"
    (cd ./frontend && npx ng build)
}

run_test_coverage() {
    step "Frontend test coverage (≥90% coverage)"
    (cd ./frontend && npx ng test --coverage --watch=false)
}

# ── Python checks ─────────────────────────────────────────────────────────────
run_py_sync() {
    step "Python dependencies (locked)"
    (cd ai-service && uv sync --locked)
    pass "py sync"
}

run_py_fmt() {
    step "Python format check"
    (cd ai-service && uv run --locked ruff format --check .)
    pass "py fmt"
}

run_py_lint() {
    step "Python lint"
    (cd ai-service && uv run --locked ruff check .)
    pass "py lint"
}

run_py_test() {
    step "Python tests (≥75% coverage)"
    (cd ai-service && uv run --locked pytest)
    pass "py tests"
}

# ── Pipeline ──────────────────────────────────────────────────────────────────
FILTER="${1:-all}"

# Handle database-only reset before installing other dependencies.
if [ "$FILTER" = "reset-db" ]; then
    : "${DATABASE_URL:?DATABASE_URL must be set}"
    ensure_tool sqlx \
        "cargo install sqlx-cli --no-default-features --features postgres --locked"
    run_reset_db
    exit 0
fi

# Reset only when explicitly requested with --reset.
if [ "$FILTER" = "--reset" ]; then
    : "${DATABASE_URL:?DATABASE_URL must be set}"
    run_reset_db
elif [ "$FILTER" != "fmt" ] && [ "$FILTER" != "clippy" ]; then
    : "${DATABASE_URL:?DATABASE_URL must be set}"
    ensure_tool sqlx \
        "cargo install sqlx-cli --no-default-features --features postgres --locked"
fi

# Install dependencies for pipeline commands.
case "$FILTER" in
    fmt|clippy)
        ;;
    *)
        ensure_dependencies
        ;;
esac

# --reset resets the database first, then continues through the full pipeline.
if [ "$FILTER" = "--reset" ]; then
    FILTER="all"
fi

case "$FILTER" in
    fmt)
        run_fmt
        ;;

    clippy)
        run_clippy
        ;;

    python)
        run_migrate
        run_py_sync
        run_py_fmt
        run_py_lint
        run_py_test
        ;;

    test)
        run_migrate
        run_unit_coverage
        run_integration_coverage
        ;;

    openapi)
        run_migrate
        run_openapi
        ;;

    all)
        run_migrate
        run_fmt
        run_clippy
        run_build
        run_unit_coverage
        run_integration_coverage
        run_py_sync
        run_py_fmt
        run_py_lint
        run_py_test
        run_openapi
        run_nglint
        run_prettier_check
        run_angular_build
        run_test_coverage

        echo -e "\n${GREEN}✓ All CI checks passed.${NC}"
        ;;

    *)
        echo "Usage: $0 [--reset|reset-db|fmt|clippy|python|test|openapi|all]"
        exit 1
        ;;
esac