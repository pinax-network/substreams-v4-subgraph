#!/usr/bin/env bash
set -euo pipefail

if (( $# != 2 )); then
  echo "usage: $0 <root|child> <output-dir>" >&2
  exit 1
fi

mode="$1"
output_dir="$2"
case "$mode" in root|child) ;; *) echo "mode must be root or child" >&2; exit 1 ;; esac

oracle_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$oracle_root/.." && pwd)"
runtime_root="$oracle_root/.runtime/dumps"
compose=(docker compose -f "$oracle_root/docker-compose.yml")
graphql_port="${ORACLE_GRAPHQL_PORT:-18100}"
status_port="${ORACLE_STATUS_PORT:-18130}"
deployment="$(jq -r ".$mode.deployment" "$repository_root/fixtures/oracle-ranges.json")"
name="$(jq -r ".$mode.name" "$repository_root/fixtures/oracle-ranges.json")"
target_block="$(jq -r ".$mode.end_block" "$repository_root/fixtures/oracle-ranges.json")"
target_hash="$(jq -r ".$mode.end_hash" "$repository_root/fixtures/oracle-ranges.json")"

test ! -e "$output_dir" || { echo "output already exists: $output_dir" >&2; exit 1; }
mkdir -p "$runtime_root"
dump_dir="$(mktemp -d "$runtime_root/$mode.XXXXXX")"
dump_name="$(basename "$dump_dir")"

meta_response="$(
  curl -fsS -H 'content-type: application/json' \
    --data '{"query":"{ _meta { block { number hash } hasIndexingErrors } }"}' \
    "http://127.0.0.1:$graphql_port/subgraphs/name/$name"
)"
test "$(jq -r '.data._meta.block.number' <<<"$meta_response")" = "$target_block"
test "$(jq -r '.data._meta.block.hash' <<<"$meta_response")" = "$target_hash"
test "$(jq -r '.data._meta.hasIndexingErrors' <<<"$meta_response")" = "false"

"${compose[@]}" exec -T graph-node \
  graphman --config /tmp/config.toml --node-id oracle dump "$deployment" "/dumps/$dump_name"
"$oracle_root/canonicalize-dump.sh" "$dump_dir" "$target_block" "$output_dir"

query="$(sed "s/__BLOCK_NUMBER__/$target_block/g" "$oracle_root/graphql-query.graphql")"
curl -fsS -H 'content-type: application/json' \
  --data "$(jq -cn --arg query "$query" '{query:$query}')" \
  "http://127.0.0.1:$graphql_port/subgraphs/name/$name" \
  | jq -S . > "$output_dir/graphql.json"
jq -e '.errors == null' "$output_dir/graphql.json" >/dev/null

poi_query="query { proofOfIndexing(subgraph: \"$deployment\", blockNumber: $target_block, blockHash: \"$target_hash\", indexer: \"0x0000000000000000000000000000000000000000\") }"
curl -fsS -H 'content-type: application/json' \
  --data "$(jq -cn --arg query "$poi_query" '{query:$query}')" \
  "http://127.0.0.1:$status_port/graphql" \
  | jq -S . > "$output_dir/poi.json"
jq -e '.errors == null and .data.proofOfIndexing != null' "$output_dir/poi.json" >/dev/null

(
  cd "$output_dir"
  if command -v sha256sum >/dev/null 2>&1; then
    find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
  else
    find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 shasum -a 256 > SHA256SUMS
  fi
)

echo "canonical $mode oracle fixture written to $output_dir"
