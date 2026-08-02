#!/usr/bin/env bash
set -euo pipefail

oracle_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
docker compose -f "$oracle_root/docker-compose.yml" down
