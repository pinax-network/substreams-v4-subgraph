#!/usr/bin/env bash
set -euo pipefail

if (( $# < 1 || $# > 2 )); then
  echo "usage: $0 <native-parquet-dump-dir> [new-subgraph-name]" >&2
  exit 1
fi

dump_dir=$(cd "$1" && pwd)
name=${2:-oracle/native-parquet-fixture}
restore_mode=${RESTORE_MODE:-force}
oracle_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repository_root=$(cd "$oracle_root/.." && pwd)
compose=(docker compose -f "$oracle_root/docker-compose.yml")
graphql_port=${ORACLE_GRAPHQL_PORT:-18100}
status_port=${ORACLE_STATUS_PORT:-18130}
metadata=$dump_dir/metadata.json
deployment=$(jq -r '.child.deployment' "$repository_root/fixtures/oracle-ranges.json")
head_block=$(jq -r '.child.end_block' "$repository_root/fixtures/oracle-ranges.json")
head_hash=$(jq -r '.child.end_hash' "$repository_root/fixtures/oracle-ranges.json")

: "${PINAX_API_KEY:?set PINAX_API_KEY before starting the oracle}"

if [[ "$restore_mode" != "force" && "$restore_mode" != "replace" ]]; then
  echo "RESTORE_MODE must be force or replace" >&2
  exit 1
fi

jq -e --arg deployment "$deployment" --argjson block "$head_block" --arg hash "${head_hash#0x}" '
  .version == 1
  and .deployment == $deployment
  and .head_block.number == $block
  and .head_block.hash == $hash
' "$metadata" >/dev/null

"${compose[@]}" stop graph-node
restart_graph_node=true
trap 'if [[ "$restart_graph_node" = true ]]; then "${compose[@]}" start graph-node >/dev/null; fi' EXIT
if [[ "$restore_mode" = "replace" ]]; then
  restore_commands="graphman --config /tmp/config.toml --node-id oracle restore /native-dump --shard primary --name '$name' --replace"
else
  restore_commands="graphman --config /tmp/config.toml --node-id oracle create '$name'
   graphman --config /tmp/config.toml --node-id oracle restore /native-dump --shard primary --name '$name' --force"
fi
"${compose[@]}" run --rm --no-deps -T \
  --volume "$dump_dir:/native-dump:ro" \
  --entrypoint /bin/sh graph-node -ec \
  "envsubst < /config/config.toml.template > /tmp/config.toml
   $restore_commands
   graphman --config /tmp/config.toml --node-id oracle pause '$deployment'"
"${compose[@]}" start graph-node
restart_graph_node=false

deadline=$((SECONDS + 120))
until curl -fsS "http://127.0.0.1:$status_port/" >/dev/null 2>&1; do
  if (( SECONDS >= deadline )); then
    echo "Graph Node did not become ready after restoring native Parquet" >&2
    exit 1
  fi
  sleep 2
done

response=$(curl -fsS -H 'content-type: application/json' \
  --data '{"query":"{ _meta { block { number hash } deployment hasIndexingErrors } }"}' \
  "http://127.0.0.1:$graphql_port/subgraphs/name/$name")
jq -e --arg deployment "$deployment" --argjson block "$head_block" --arg hash "$head_hash" '
  .errors == null
  and .data._meta.deployment == $deployment
  and .data._meta.block.number == $block
  and .data._meta.block.hash == $hash
  and .data._meta.hasIndexingErrors == false
' <<<"$response" >/dev/null

echo "$deployment restored from native Parquet and paused at $head_block ($head_hash)"
