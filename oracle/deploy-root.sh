#!/usr/bin/env bash
set -euo pipefail

oracle_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$oracle_root/.." && pwd)"
compose=(docker compose -f "$oracle_root/docker-compose.yml")
graphql_port="${ORACLE_GRAPHQL_PORT:-18100}"
metrics_port="${ORACLE_METRICS_PORT:-18140}"
deployment="$(jq -r '.root.deployment' "$repository_root/fixtures/oracle-ranges.json")"
name="$(jq -r '.root.name' "$repository_root/fixtures/oracle-ranges.json")"
target_block="$(jq -r '.root.end_block' "$repository_root/fixtures/oracle-ranges.json")"
target_hash="$(jq -r '.root.end_hash' "$repository_root/fixtures/oracle-ranges.json")"

"$oracle_root/import-artifacts.sh"
existing_deployment="$(
  curl -fsS -H 'content-type: application/json' \
    --data '{"query":"{ _meta { deployment } }"}' \
    "http://127.0.0.1:$graphql_port/subgraphs/name/$name" 2>/dev/null \
    | jq -r '.data._meta.deployment // empty' || true
)"
if [[ "$existing_deployment" != "$deployment" ]]; then
  "${compose[@]}" exec -T graph-node \
    graphman --config /tmp/config.toml --node-id oracle deploy "$name" "$deployment"
else
  echo "root oracle deployment already exists; preserving its current assignment"
fi

deadline=$((SECONDS + 900))
current_block=0
while (( SECONDS < deadline )); do
  response="$(
    curl -fsS -H 'content-type: application/json' \
      --data '{"query":"{ _meta { block { number hash } hasIndexingErrors } }"}' \
      "http://127.0.0.1:$graphql_port/subgraphs/name/$name" 2>/dev/null || true
  )"
  current_block="$(jq -r '.data._meta.block.number // 0' <<<"$response" 2>/dev/null || echo 0)"
  metrics_block="$(
    curl -fsS "http://127.0.0.1:$metrics_port/metrics" 2>/dev/null \
      | awk -v deployment="$deployment" \
        '$0 ~ "^deployment_head\\{deployment=\\\"" deployment "\\\"" { print int($2); exit }' \
      || true
  )"
  if [[ "$metrics_block" =~ ^[0-9]+$ ]] && (( metrics_block > current_block )); then
    current_block="$metrics_block"
  fi
  if (( current_block >= target_block )); then
    break
  fi
  sleep 2
done

if (( current_block < target_block )); then
  echo "root oracle did not reach block $target_block within 15 minutes (last: $current_block)" >&2
  exit 1
fi

"$oracle_root/checkpoint-deployment.sh" "$deployment" "$name" "$target_block" "$target_hash"

echo "root oracle is paused at $target_block ($target_hash)"
