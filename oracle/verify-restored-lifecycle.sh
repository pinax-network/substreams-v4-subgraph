#!/usr/bin/env bash
set -euo pipefail

if (( $# != 2 )); then
  echo "usage: $0 <local-restored-schema> <subgraph-name>" >&2
  exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

: "${KUBECONFIG:?set KUBECONFIG to an authorized read-only cluster config}"
: "${PINAX_API_KEY:?set PINAX_API_KEY for the disposable local oracle}"

local_schema=$1
name=$2
fixture=fixtures/resume-range.json
deployment=$(jq -er '.deployment' "$fixture")
seed_block=$(jq -er '.segments[-1].end_block' "$fixture")
seed_hash=$(jq -er '.segments[-1].end_hash' "$fixture")
target_block=$(jq -er '.lifecycle_target.end_block' "$fixture")
target_hash=$(jq -er '.lifecycle_target.end_hash' "$fixture")
compose=(docker compose -f oracle/docker-compose.yml)
started_at=$SECONDS

if [[ ! "$local_schema" =~ ^sgd[0-9]+$ ]]; then
  echo "local schema must match sgd followed by digits" >&2
  exit 1
fi

actual=$("${compose[@]}" exec -T postgres \
  env 'PGOPTIONS=-c default_transaction_read_only=on' \
  psql -U graph -d graph -AtX -v ON_ERROR_STOP=1 \
  -c "SELECT subgraph FROM deployment_schemas WHERE name = '$local_schema';")
if [[ "$actual" != "$deployment" ]]; then
  echo "$local_schema contains '$actual', expected '$deployment'" >&2
  exit 1
fi

work_dir=$(mktemp -d /tmp/substreams-v4-lifecycle.XXXXXX)
trap 'rm -rf "$work_dir"' EXIT

query_checkpoint() {
  ./oracle/query-checkpoint.sh "$name" "$1" "$2"
}

resume_to_target() {
  "${compose[@]}" exec -T graph-node \
    graphman --config /tmp/config.toml --node-id oracle resume "$deployment" >/dev/null
  local deadline=$((SECONDS + 300)) current=0 response
  while (( SECONDS < deadline )); do
    response=$(curl -fsS -H 'content-type: application/json' \
      --data '{"query":"{ _meta { block { number } } }"}' \
      "http://127.0.0.1:${ORACLE_GRAPHQL_PORT:-18100}/subgraphs/name/$name" 2>/dev/null || true)
    current=$(jq -r '.data._meta.block.number // 0' <<<"$response" 2>/dev/null || echo 0)
    if (( current >= target_block )); then break; fi
    sleep 1
  done
  if (( current < target_block )); then
    echo "local oracle reached only $current, expected at least $target_block" >&2
    exit 1
  fi
  ./oracle/checkpoint-deployment.sh "$deployment" "$name" "$target_block" "$target_hash" >/dev/null
}

verify_target_rows() {
  SEED_BLOCK=$seed_block START_BLOCK=$((seed_block + 1)) END_BLOCK=$target_block \
    PARITY_REPORT="$1" ./scripts/verify-restored-parity.sh "$local_schema" >/dev/null
}

# Always start at the exact restored Parquet head.
./oracle/checkpoint-deployment.sh "$deployment" "$name" "$seed_block" "$seed_hash" >/dev/null
query_checkpoint "$seed_block" "$work_dir/seed.json"

resume_to_target
query_checkpoint "$target_block" "$work_dir/target-first.json"
verify_target_rows "$work_dir/target-first-physical.json"

"${compose[@]}" restart graph-node >/dev/null
deadline=$((SECONDS + 120))
until curl -fsS "http://127.0.0.1:${ORACLE_STATUS_PORT:-18130}/" >/dev/null 2>&1; do
  if (( SECONDS >= deadline )); then
    echo "Graph Node did not become ready after restart" >&2
    exit 1
  fi
  sleep 1
done
query_checkpoint "$target_block" "$work_dir/target-restarted.json"
cmp "$work_dir/target-first.json" "$work_dir/target-restarted.json"

./oracle/checkpoint-deployment.sh "$deployment" "$name" "$seed_block" "$seed_hash" >/dev/null
query_checkpoint "$seed_block" "$work_dir/seed-rewound.json"
cmp "$work_dir/seed.json" "$work_dir/seed-rewound.json"

future_query=$(
  for table in pool_manager bundle token pool tick uniswap_day_data pool_day_data \
    pool_hour_data token_day_data token_hour_data position arrakis_hook 'poi2$'; do
    printf "SELECT '%s', count(*) FROM %s.\"%s\" WHERE lower(block_range) BETWEEN %d AND %d UNION ALL " \
      "$table" "$local_schema" "$table" "$((seed_block + 1))" "$target_block"
  done
  for table in transaction swap modify_liquidity subscribe unsubscribe transfer; do
    printf "SELECT '%s', count(*) FROM %s.\"%s\" WHERE \"block$\" BETWEEN %d AND %d UNION ALL " \
      "$table" "$local_schema" "$table" "$((seed_block + 1))" "$target_block"
  done
  printf "SELECT 'sentinel', 0"
)
"${compose[@]}" exec -T postgres env 'PGOPTIONS=-c default_transaction_read_only=on' \
  psql -U graph -d graph -AtX -F $'\t' -v ON_ERROR_STOP=1 -c "$future_query" \
  > "$work_dir/reverted-counts.tsv"
awk -F '\t' '$1 != "sentinel" && $2 != 0 { bad=1 } END { exit bad }' \
  "$work_dir/reverted-counts.tsv"

resume_to_target
query_checkpoint "$target_block" "$work_dir/target-replayed.json"
cmp "$work_dir/target-first.json" "$work_dir/target-replayed.json"
verify_target_rows "$work_dir/target-replayed-physical.json"

report=$work_dir/report.json
jq -n \
  --arg deployment "$deployment" \
  --arg schema "$local_schema" \
  --arg name "$name" \
  --argjson seed_block "$seed_block" \
  --argjson target_block "$target_block" \
  --argjson elapsed_seconds "$((SECONDS - started_at))" \
  --slurpfile physical "$work_dir/target-replayed-physical.json" '
  {
    deployment:$deployment,
    local_schema:$schema,
    subgraph_name:$name,
    seed_block:$seed_block,
    target_block:$target_block,
    restart_graphql_exact:true,
    rewind_graphql_exact:true,
    reverted_future_rows:0,
    replay_graphql_exact:true,
    physical:$physical[0],
    elapsed_seconds:$elapsed_seconds,
    status:"complete"
  }
' > "$report"

if [[ -n "${LIFECYCLE_REPORT:-}" ]]; then
  mkdir -p "$(dirname "$LIFECYCLE_REPORT")"
  cp "$report" "$LIFECYCLE_REPORT"
fi
jq . "$report"
