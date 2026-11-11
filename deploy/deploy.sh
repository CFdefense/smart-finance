#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

COMPOSE="compose -f deploy/docker-compose.yml --env-file .env"

git switch main
git pull origin main

docker $COMPOSE up -d --wait db

sqlx migrate run

docker $COMPOSE up -d --build ai-service