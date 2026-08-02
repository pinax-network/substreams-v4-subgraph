#!/usr/bin/env bash
set -euo pipefail

if (( $# != 4 )); then
  echo "usage: $0 <deployment> <subgraph-name> <block-number> <block-hash>" >&2
  exit 1
fi

deployment="$1"
name="$2"
target_block="$3"
target_hash="$4"
oracle_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compose=(docker compose -f "$oracle_root/docker-compose.yml")
graphql_port="${ORACLE_GRAPHQL_PORT:-18100}"
status_port="${ORACLE_STATUS_PORT:-18130}"

"${compose[@]}" exec -T graph-node \
  graphman --config /tmp/config.toml --node-id oracle pause "$deployment"

paused_response="$(
  curl -fsS -H 'content-type: application/json' \
    --data '{"query":"{ _meta { block { number hash } hasIndexingErrors } }"}' \
    "http://127.0.0.1:$graphql_port/subgraphs/name/$name"
)"
paused_block="$(jq -r '.data._meta.block.number' <<<"$paused_response")"
if (( paused_block > target_block )); then
  "${compose[@]}" stop graph-node
  restart_graph_node=true
  trap 'if [[ "$restart_graph_node" = true ]]; then "${compose[@]}" start graph-node >/dev/null; fi' EXIT
  "${compose[@]}" run --rm --no-deps -T --entrypoint /bin/sh graph-node -ec \
    "envsubst < /config/config.toml.template > /tmp/config.toml
     graphman --config /tmp/config.toml --node-id oracle rewind --force --sleep 0 --block-number '$target_block' --block-hash '$target_hash' '$deployment'
     graphman --config /tmp/config.toml --node-id oracle pause '$deployment'"
  "${compose[@]}" start graph-node
  restart_graph_node=false

  deadline=$((SECONDS + 120))
  until curl -fsS "http://127.0.0.1:$status_port/" >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then
      echo "Graph Node did not become ready after checkpointing $deployment" >&2
      exit 1
    fi
    sleep 2
  done
elif (( paused_block < target_block )); then
  echo "$deployment paused before target block $target_block (current: $paused_block)" >&2
  exit 1
fi

deadline=$((SECONDS + 120))
response=""
while (( SECONDS < deadline )); do
  response="$(
    curl -fsS -H 'content-type: application/json' \
      --data '{"query":"{ _meta { block { number hash } hasIndexingErrors } }"}' \
      "http://127.0.0.1:$graphql_port/subgraphs/name/$name" 2>/dev/null || true
  )"
  if [[ "$(jq -r '.data._meta.block.number // empty' <<<"$response" 2>/dev/null || true)" = "$target_block" ]]; then
    break
  fi
  sleep 2
done

test "$(jq -r '.data._meta.block.number' <<<"$response")" = "$target_block"
test "$(jq -r '.data._meta.block.hash' <<<"$response")" = "$target_hash"
test "$(jq -r '.data._meta.hasIndexingErrors' <<<"$response")" = "false"

echo "$deployment is paused at $target_block ($target_hash)"
