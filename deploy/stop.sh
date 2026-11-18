#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

docker compose -f deploy/docker-compose.yml --env-file .env down --rmi local
