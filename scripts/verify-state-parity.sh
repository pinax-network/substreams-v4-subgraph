#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

: "${KUBECONFIG:?set KUBECONFIG to an authorized read-only cluster config}"
: "${SUBSTREAMS_API_TOKEN:?set SUBSTREAMS_API_TOKEN for the Base endpoint}"

kube_namespace=${KUBE_NAMESPACE:-subgraphs}
postgres_pod=${POSTGRES_POD:-univ4base-postgres-0}
postgres_user=${POSTGRES_USER:-graph}
postgres_db=${POSTGRES_DB:-graph}
graph_node_workload_kind=${GRAPH_NODE_WORKLOAD_KIND:-deployment}
graph_node_workload=${GRAPH_NODE_WORKLOAD:-graph-node-basegiant-0}
schema=${GRAPH_SCHEMA:-sgd1246}
deployment=${DEPLOYMENT_ID:-Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB}
endpoint=${ENDPOINT:-base-substreams-tier1-prod.kan-sst2.pinax.io:443}
seed_block=${SEED_BLOCK:-26990278}
start_block=${START_BLOCK:-26990279}
end_block=${END_BLOCK:-26990520}
stop_block=$((end_block + 1))

if [[ ! "$schema" =~ ^sgd[0-9]+$ ]]; then
    echo "GRAPH_SCHEMA must match sgd followed by digits" >&2
    exit 1
fi

replicas=$(kubectl -n "$kube_namespace" get "$graph_node_workload_kind" "$graph_node_workload" \
    -o jsonpath='{.spec.replicas}')
if [[ "$replicas" != "0" ]]; then
    echo "$graph_node_workload_kind/$graph_node_workload must have zero replicas before it can be used as an oracle" >&2
    exit 1
fi

actual_deployment=$(kubectl -n "$kube_namespace" exec "$postgres_pod" -- \
    psql -U "$postgres_user" -d "$postgres_db" -AtX -v ON_ERROR_STOP=1 \
    -c "SET default_transaction_read_only=on; SELECT subgraph FROM deployment_schemas WHERE name = '$schema';")
actual_deployment=${actual_deployment##*$'\n'}
if [[ "$actual_deployment" != "$deployment" ]]; then
    echo "$schema is deployment '$actual_deployment', expected '$deployment'" >&2
    exit 1
fi

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
    cargo build --locked --target wasm32-unknown-unknown --release
    cargo build --locked --features native --bin state-replay
fi
replay_command=(target/debug/state-replay)
if [[ -n "${SNAPSHOT_OUTPUT:-}" ]]; then
    replay_command+=(--snapshot "$SNAPSHOT_OUTPUT")
fi

oracle_export=$(mktemp /tmp/substreams-v4-oracle-export.XXXXXX)
trap 'rm -f "$oracle_export"' EXIT
export_complete=0
for attempt in 1 2 3; do
    : > "$oracle_export"
    if kubectl -n "$kube_namespace" exec -i "$postgres_pod" -- \
        psql -U "$postgres_user" -d "$postgres_db" -AtX -v ON_ERROR_STOP=1 \
        -v schema="$schema" \
        -v seed_block="$seed_block" \
        -v start_block="$start_block" \
        -v end_block="$end_block" \
        < scripts/export-state-parity.sql > "$oracle_export" && \
        jq -Rse '
            split("\n") | map(fromjson? | select(. != null)) as $rows |
            ($rows | map(select(has("@export_complete"))) | last."@export_complete") as $manifest |
            $manifest != null and
            ([$rows[] | select(has("@table"))] | length) == $manifest.table_records and
            ([$rows[] | select(has("@poi_seed"))] | length) == $manifest.poi_seed_records and
            ([$rows[] | select(has("@poi_expected"))] | length) == $manifest.poi_expected_records and
            ([$rows[] | select(has("@seed_version"))] | length) == $manifest.seed_records and
            ([$rows[] | select(has("@expected"))] | length) == $manifest.expected_records
        ' "$oracle_export" >/dev/null; then
        export_complete=1
        break
    fi
    echo "oracle export attempt $attempt was incomplete" >&2
done
if (( export_complete == 0 )); then
    echo "oracle export failed its completeness manifest after 3 attempts" >&2
    exit 1
fi

set +e
result=$(
    {
        cat "$oracle_export"
        substreams run -e "$endpoint" substreams.yaml map_events \
            -s "$start_block" -t "$stop_block" -o jsonl
    } | "${replay_command[@]}"
)
replay_status=$?
set -e

printf '%s\n' "$result" | jq
if ((replay_status != 0)); then
    exit "$replay_status"
fi
printf '%s\n' "$result" | jq -e '
    .mismatch_count == 0 and
    .poi_checked == true and
    .expected_entities == .changed_entities and
    .expected_entities > 0
' >/dev/null
