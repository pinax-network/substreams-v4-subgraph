#!/usr/bin/env bash
set -euo pipefail

oracle_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
graphql_port="${ORACLE_GRAPHQL_PORT:-18100}"

test -n "${PINAX_API_KEY:-}" || {
  echo "PINAX_API_KEY is required for the same Firehose + archive RPC provider class used in production" >&2
  exit 1
}

mkdir -p "$oracle_root/.runtime/dumps"
docker compose -f "$oracle_root/docker-compose.yml" up -d --wait
"$oracle_root/import-artifacts.sh"

echo "Graph Node v0.44 oracle is ready on http://127.0.0.1:$graphql_port"
