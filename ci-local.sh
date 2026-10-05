#!/usr/bin/env bash
# ci-local.sh — run the CI pipeline locally (same steps as .github/workflows/ci.yml).
# Prerequisites: cargo, sqlx-cli, cargo-tarpaulin installed on your machine.
# Database: run `docker compose up -d db` first.
#
# Usage:
#   ./ci-local.sh          # full pipeline
#   ./ci-local.sh fmt      # formatting only
#   ./ci-local.sh clippy   # clippy only
#   ./ci-local.sh test     # tests + coverage only

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
pass() { echo -e "${GREEN}✓ $1${NC}"; }
fail() { echo -e "${RED}✗ $1${NC}"; exit 1; }
step() { echo -e "\n${YELLOW}▶ $1${NC}"; }

[ -f .env ] && { set -a; source .env; set +a; } || fail ".env not found — copy .env.example"
: "${DATABASE_URL:?DATABASE_URL must be set in .env}"

FILTER="${1:-all}"

run_migrate()             { step "Migrations";                   sqlx migrate run && pass "migrations"; }
run_fmt()                 { step "Format check";                 cargo fmt --check && pass "fmt"; }
run_clippy()              { step "Clippy";                       cargo clippy -- -D warnings && pass "clippy"; }
run_build()               { step "Build";                        cargo build --locked && pass "build"; }
run_unit_coverage()       { step "Unit tests (≥90% coverage)";   cargo tarpaulin --run-types Tests --fail-under 90 --out Stdout && pass "unit coverage"; }
run_integration_coverage(){ step "Integration tests (≥75% coverage)"; cargo tarpaulin --run-types Bins --fail-under 75 --out Stdout Html --output-dir docs/coverage && pass "integration coverage"; }
run_openapi()             { step "Generate openapi.json";        cargo run --bin gen-openapi && pass "openapi.json"; }

case "$FILTER" in
  fmt)     run_fmt ;;
  clippy)  run_clippy ;;
  test)    run_migrate; run_unit_coverage; run_integration_coverage ;;
  all)
    run_migrate
    run_fmt
    run_clippy
    run_build
    run_unit_coverage
    run_integration_coverage
    run_openapi
    echo -e "\n${GREEN}✓ All CI checks passed.${NC}" ;;
  *)  echo "Usage: $0 [fmt|clippy|test|all]"; exit 1 ;;
esac
