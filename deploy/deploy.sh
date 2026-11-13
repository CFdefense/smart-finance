#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

COMPOSE="compose -f deploy/docker-compose.yml --env-file .env"

echo "==> Switching to main"
git switch main

echo "==> Pulling latest changes"
git pull --ff-only origin main

echo "==> Starting the database and waiting until it is healthy"
docker $COMPOSE up -d --wait db

echo "==> Running database migrations"
sqlx migrate run

echo "==> Building the backend (compiles against the migrated database)"
# The backend build uses host networking to reach Postgres on 127.0.0.1:5432.
# Newer Compose versions refuse that from `compose build` ("pass
# --allow=network.host"), so on failure retry through buildx bake, which lets
# us grant the permission. Same build definition either way.
if ! docker $COMPOSE build backend; then
  echo "==> Retrying the backend build with host networking explicitly allowed"
  docker $COMPOSE build --print backend \
    | docker buildx bake --file - --allow network.host --allow "fs.read=$PWD"
fi

echo "==> Building the other images"
docker $COMPOSE build ai-service frontend

echo "==> Starting all services"
docker $COMPOSE up -d

echo "==> Done"
docker $COMPOSE ps
