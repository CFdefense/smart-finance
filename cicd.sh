#!/usr/bin/env bash
# cicd.sh — single source of truth for the CI/CD pipeline.
# Used by .github/workflows/ci.yml and for local pre-push checks.
#
# Database: run `docker compose up -d db` first (local), or set DATABASE_URL (CI).
#
# Usage:
#   ./cicd.sh              # full pipeline
#   ./cicd.sh fmt          # formatting only
#   ./cicd.sh clippy       # clippy only
#   ./cicd.sh test         # tests + coverage only
#   ./cicd.sh openapi      # generate openapi.json only

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
pass() { echo -e "${GREEN}✓ $1${NC}"; }
fail() { echo -e "${RED}✗ $1${NC}"; exit 1; }
step() { echo -e "\n${YELLOW}▶ $1${NC}"; }
info() { echo -e "${YELLOW}  $1${NC}"; }

# ── Ensure required tools are installed ───────────────────────────────────────
ensure_tool() {
    local cmd="$1" install_cmd="$2"
    if ! command -v "$cmd" &>/dev/null; then
        info "$cmd not found — installing..."
        eval "$install_cmd"
    fi
}

ensure_tool sqlx      "cargo install sqlx-cli --no-default-features --features postgres --locked"
ensure_tool cargo-tarpaulin "cargo install cargo-tarpaulin --locked"

# Load .env when running locally; in CI DATABASE_URL is set via env
[ -f .env ] && { set -a; source .env; set +a; }
: "${DATABASE_URL:?DATABASE_URL must be set}"

FILTER="${1:-all}"

run_migrate()             { step "Migrations";                        sqlx migrate run && pass "migrations"; }
run_fmt()                 { step "Format check";                      cargo fmt --check && pass "fmt"; }
run_clippy()              { step "Clippy";                            cargo clippy -- -D warnings && pass "clippy"; }
run_build()               { step "Build";                             cargo build --locked && pass "build"; }
run_unit_coverage()       { step "Unit tests (≥90% coverage)";        cargo tarpaulin --run-types Tests --fail-under 90 --out Stdout && pass "unit coverage"; }
run_integration_coverage(){ step "Integration tests (≥75% coverage)"; cargo tarpaulin --run-types Bins --fail-under 75 --out Stdout Html --output-dir docs/coverage && pass "integration coverage"; }
run_openapi()             { step "Generate openapi.json";             cargo run --bin gen-openapi && pass "openapi.json"; }

case "$FILTER" in
  fmt)     run_fmt ;;
  clippy)  run_clippy ;;
  test)    run_migrate; run_unit_coverage; run_integration_coverage ;;
  openapi) run_openapi ;;
  all)
    run_migrate
    run_fmt
    run_clippy
    run_build
    run_unit_coverage
    run_integration_coverage
    run_openapi
    echo -e "\n${GREEN}✓ All CI checks passed.${NC}" ;;
  *)  echo "Usage: $0 [fmt|clippy|test|openapi|all]"; exit 1 ;;
esac
