#!/usr/bin/env bash
set -euo pipefail

if (( $# != 3 )); then
  echo "usage: $0 <subgraph-name> <head-block> <new-output-file>" >&2
  exit 1
fi

name=$1
head_block=$2
output=$3
oracle_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repository_root=$(cd "$oracle_root/.." && pwd)
graphql_port=${ORACLE_GRAPHQL_PORT:-18100}
deployment=$(jq -er '.deployment' "$repository_root/fixtures/resume-range.json")

if [[ ! "$head_block" =~ ^[0-9]+$ ]]; then
  echo "head block must be numeric" >&2
  exit 1
fi
if [[ -e "$output" ]]; then
  echo "output path already exists: $output" >&2
  exit 1
fi

query=$(sed "s/__BLOCK_NUMBER__/$head_block/g" "$oracle_root/graphql-query.graphql")
payload=$(jq -cn --arg query "$query" '{query:$query}')
temporary=$output.tmp
trap 'rm -f "$temporary"' EXIT
curl -fsS -H 'content-type: application/json' --data "$payload" \
  "http://127.0.0.1:$graphql_port/subgraphs/name/$name" | jq -S . > "$temporary"
jq -e --arg deployment "$deployment" --argjson block "$head_block" '
  .errors == null and
  .data._meta.deployment == $deployment and
  .data._meta.block.number == $block and
  .data._meta.hasIndexingErrors == false
' "$temporary" >/dev/null
mv "$temporary" "$output"
trap - EXIT
