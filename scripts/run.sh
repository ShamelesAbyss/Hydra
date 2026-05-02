#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p data logs
set -a; [ -f .env ] && . ./.env; set +a
exec ./target/release/hydra "$@"
