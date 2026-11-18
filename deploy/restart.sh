#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

./deploy/stop.sh
./deploy/deploy.sh "$@"
