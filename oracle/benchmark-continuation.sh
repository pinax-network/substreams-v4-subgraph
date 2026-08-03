#!/usr/bin/env bash
set -euo pipefail

if (( $# != 6 )); then
  echo "usage: $0 <local-schema> <subgraph-name> <seed-block> <seed-hash> <target-block> <target-hash>" >&2
  exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

: "${PINAX_API_KEY:?set PINAX_API_KEY for the disposable local oracle}"

schema=$1
name=$2
seed_block=$3
seed_hash=$4
target_block=$5
target_hash=$6
deployment=Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB
graphql_port=${ORACLE_GRAPHQL_PORT:-18100}
compose=(docker compose -f oracle/docker-compose.yml)

if [[ ! "$schema" =~ ^sgd[0-9]+$ ]]; then
  echo "local schema must match sgd followed by digits" >&2
  exit 1
fi
for block in "$seed_block" "$target_block"; do
  if [[ ! "$block" =~ ^[0-9]+$ ]]; then
    echo "block numbers must be numeric" >&2
    exit 1
  fi
done
for hash in "$seed_hash" "$target_hash"; do
  if [[ ! "$hash" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
    echo "block hashes must be 0x-prefixed 32-byte hex values" >&2
    exit 1
  fi
done
if (( target_block <= seed_block )); then
  echo "target block must be after seed block" >&2
  exit 1
fi
timeout_seconds=${CONTINUATION_TIMEOUT_SECONDS:-3600}
if [[ ! "$timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo "CONTINUATION_TIMEOUT_SECONDS must be a positive integer" >&2
  exit 1
fi

actual=$("${compose[@]}" exec -T postgres \
  env 'PGOPTIONS=-c default_transaction_read_only=on' \
  psql -U graph -d graph -AtX -v ON_ERROR_STOP=1 \
  -c "SELECT subgraph FROM deployment_schemas WHERE name = '$schema';")
if [[ "$actual" != "$deployment" ]]; then
  echo "$schema contains '$actual', expected '$deployment'" >&2
  exit 1
fi

work_dir=$(mktemp -d /tmp/substreams-v4-continuation.XXXXXX)
cleanup() {
  "${compose[@]}" exec -T graph-node \
    graphman --config /tmp/config.toml --node-id oracle pause "$deployment" >/dev/null 2>&1 || true
  rm -rf "$work_dir"
}
trap cleanup EXIT

./oracle/checkpoint-deployment.sh \
  "$deployment" "$name" "$seed_block" "$seed_hash" >/dev/null
./oracle/query-checkpoint.sh "$name" "$seed_block" "$work_dir/seed.json"

started_at=$SECONDS
"${compose[@]}" exec -T graph-node \
  graphman --config /tmp/config.toml --node-id oracle resume "$deployment" >/dev/null

deadline=$((SECONDS + timeout_seconds))
current=$seed_block
observed=$seed_block
while (( SECONDS < deadline )); do
  response=$(curl -fsS -H 'content-type: application/json' \
    --data '{"query":"{ _meta { block { number } hasIndexingErrors } }"}' \
    "http://127.0.0.1:$graphql_port/subgraphs/name/$name" 2>/dev/null || true)
  if ! jq -e '.data._meta.block.number != null' <<<"$response" >/dev/null 2>&1; then
    sleep 1
    continue
  fi
  current=$(jq -r '.data._meta.block.number // 0' <<<"$response" 2>/dev/null || echo 0)
  has_errors=$(jq -r '.data._meta.hasIndexingErrors' <<<"$response" 2>/dev/null || echo true)
  if [[ "$has_errors" == "true" ]]; then
    echo "Graph Node reported an indexing error at block $current" >&2
    exit 1
  fi
  status_response=$(curl -fsS -H 'content-type: application/json' \
    --data "$(jq -nc --arg deployment "$deployment" \
      '{query:"query($deployments: [String!]) { indexingStatuses(subgraphs: $deployments) { health fatalError { message block { number hash } } } }",variables:{deployments:[$deployment]}}')" \
    "http://127.0.0.1:${ORACLE_STATUS_PORT:-18130}/graphql" 2>/dev/null || true)
  health=$(jq -r '.data.indexingStatuses[0].health // "unknown"' \
    <<<"$status_response" 2>/dev/null || echo unknown)
  if [[ "$health" == "failed" ]]; then
    fatal_message=$(jq -r '.data.indexingStatuses[0].fatalError.message // "unknown fatal error"' \
      <<<"$status_response" 2>/dev/null || echo "unknown fatal error")
    fatal_block=$(jq -r '.data.indexingStatuses[0].fatalError.block.number // "unknown"' \
      <<<"$status_response" 2>/dev/null || echo unknown)
    echo "Graph Node failed at block $fatal_block: $fatal_message" >&2
    exit 1
  fi
  if (( current > observed )); then
    observed=$current
    printf 'Graph Node continuation: %d/%d blocks\n' \
      "$((observed - seed_block))" "$((target_block - seed_block))" >&2
  fi
  if (( current >= target_block )); then
    break
  fi
  sleep 1
done
if (( current < target_block )); then
  echo "Graph Node reached only $current before the continuation timeout" >&2
  exit 1
fi
reached_seconds=$((SECONDS - started_at))
if (( reached_seconds == 0 )); then
  reached_seconds=1
fi
curl -fsS "http://127.0.0.1:${ORACLE_METRICS_PORT:-18140}/metrics" \
  > "$work_dir/metrics-reached.prom"

metric_value() {
  local metric=$1
  awk -v metric="$metric" -v deployment="$deployment" '
    $1 ~ ("^" metric "(\\{|$)") && index($0, "deployment=\"" deployment "\"") {
      print $NF
      exit
    }
  ' "$work_dir/metrics-reached.prom"
}
handler_event_count=$(awk -v deployment="$deployment" '
  $1 ~ /^deployment_handler_execution_time_count\{/ &&
      index($0, "deployment=\"" deployment "\"") { total += $NF }
  END { print total + 0 }
' "$work_dir/metrics-reached.prom")
processed_blocks=$(metric_value deployment_blocks_processed_count)
processing_seconds=$(metric_value deployment_blocks_processed_secs)
trigger_count=$(metric_value deployment_trigger_processing_duration_count)
trigger_seconds=$(metric_value deployment_trigger_processing_duration_sum)
processed_blocks=${processed_blocks:-0}
processing_seconds=${processing_seconds:-0}
trigger_count=${trigger_count:-0}
trigger_seconds=${trigger_seconds:-0}
if awk "BEGIN { exit !($trigger_count == 0) }"; then
  trigger_count=$handler_event_count
fi

./oracle/checkpoint-deployment.sh \
  "$deployment" "$name" "$target_block" "$target_hash" >/dev/null
./oracle/query-checkpoint.sh "$name" "$target_block" "$work_dir/target.json"

blocks=$((target_block - seed_block))
report=$work_dir/report.json
jq -n \
  --arg deployment "$deployment" \
  --arg schema "$schema" \
  --arg name "$name" \
  --argjson seed_block "$seed_block" \
  --argjson target_block "$target_block" \
  --argjson observed_block "$observed" \
  --argjson blocks "$blocks" \
  --argjson reached_seconds "$reached_seconds" \
  --argjson processed_blocks "$processed_blocks" \
  --argjson processing_seconds "$processing_seconds" \
  --argjson trigger_count "$trigger_count" \
  --argjson handler_event_count "$handler_event_count" \
  --argjson trigger_seconds "$trigger_seconds" '
  {
    deployment:$deployment,
    local_schema:$schema,
    subgraph_name:$name,
    seed_block:$seed_block,
    target_block:$target_block,
    observed_block:$observed_block,
    blocks:$blocks,
    reached_seconds:$reached_seconds,
    blocks_per_second:($blocks / $reached_seconds),
    graph_node_metrics:{
      coverage_complete:($processed_blocks >= $blocks),
      sampled_processed_blocks:$processed_blocks,
      sampled_processing_seconds:$processing_seconds,
      sampled_trigger_count:$trigger_count,
      sampled_handler_event_count:$handler_event_count,
      trigger_processing_seconds:$trigger_seconds,
      sampled_triggers_per_second:(
        if $processed_blocks >= $blocks
        then $trigger_count / $reached_seconds
        else null
        end
      )
    },
    exact_target_checkpoint:true,
    status:"complete"
  }
' > "$report"

if [[ -n "${CONTINUATION_REPORT:-}" ]]; then
  mkdir -p "$(dirname "$CONTINUATION_REPORT")"
  cp "$report" "$CONTINUATION_REPORT"
fi
jq . "$report"
