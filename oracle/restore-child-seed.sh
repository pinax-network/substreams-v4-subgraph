#!/usr/bin/env bash
set -euo pipefail

if (( $# != 1 )); then
  echo "usage: $0 <child-seed-graphman-dump-dir>" >&2
  exit 1
fi

seed_dir="$(cd "$1" && pwd)"
oracle_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$oracle_root/.." && pwd)"
compose=(docker compose -f "$oracle_root/docker-compose.yml")
graphql_port="${ORACLE_GRAPHQL_PORT:-18100}"
status_port="${ORACLE_STATUS_PORT:-18130}"
metrics_port="${ORACLE_METRICS_PORT:-18140}"
metadata="$seed_dir/metadata.json"
deployment="$(jq -r '.child.deployment' "$repository_root/fixtures/oracle-ranges.json")"
name="$(jq -r '.child.name' "$repository_root/fixtures/oracle-ranges.json")"
seed_block="$(jq -r '.child.seed_block' "$repository_root/fixtures/oracle-ranges.json")"
seed_hash="$(jq -r '.child.seed_hash' "$repository_root/fixtures/oracle-ranges.json")"
graft_base="$(jq -r '.root.deployment' "$repository_root/fixtures/oracle-ranges.json")"
target_block="$(jq -r '.child.end_block' "$repository_root/fixtures/oracle-ranges.json")"
target_hash="$(jq -r '.child.end_hash' "$repository_root/fixtures/oracle-ranges.json")"

jq -e --arg deployment "$deployment" --arg graft_base "$graft_base" --argjson block "$seed_block" --arg hash "$seed_hash" '
  def with_prefix: if startswith("0x") then . else "0x" + . end;
  .deployment == $deployment
  and .graft_base == $graft_base
  and .head_block.number == $block
  and (.head_block.hash | with_prefix) == $hash
  and .graft_block.number == $block
  and (.graft_block.hash | with_prefix) == $hash
' \
  "$metadata" >/dev/null

"${compose[@]}" stop graph-node
restart_graph_node=true
trap 'if [[ "$restart_graph_node" = true ]]; then "${compose[@]}" start graph-node >/dev/null; fi' EXIT
"${compose[@]}" run --rm --no-deps -T \
  --volume "$seed_dir:/seed:ro" \
  --entrypoint /bin/sh graph-node -ec \
  "envsubst < /config/config.toml.template > /tmp/config.toml
   graphman --config /tmp/config.toml --node-id oracle restore /seed --shard primary --name '$name' --force
   graphman --config /tmp/config.toml --node-id oracle pause '$deployment'"
"${compose[@]}" start graph-node
restart_graph_node=false

deadline=$((SECONDS + 120))
until curl -fsS "http://127.0.0.1:$status_port/" >/dev/null 2>&1; do
  if (( SECONDS >= deadline )); then
    echo "Graph Node did not become ready after restoring the child seed" >&2
    exit 1
  fi
  sleep 2
done
"${compose[@]}" exec -T graph-node \
  graphman --config /tmp/config.toml --node-id oracle resume "$deployment"

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
  if (( current_block >= target_block )); then break; fi
  sleep 2
done
if (( current_block < target_block )); then
  echo "child oracle did not reach block $target_block within 15 minutes (last: $current_block)" >&2
  exit 1
fi

"$oracle_root/checkpoint-deployment.sh" "$deployment" "$name" "$target_block" "$target_hash"

echo "child oracle is paused at $target_block ($target_hash)"
