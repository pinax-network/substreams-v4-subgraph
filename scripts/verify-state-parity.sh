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

cargo build --locked --target wasm32-unknown-unknown --release
cargo build --locked --features native --bin state-replay

set +e
result=$(
    {
        kubectl -n "$kube_namespace" exec -i "$postgres_pod" -- \
            psql -U "$postgres_user" -d "$postgres_db" -AtX -v ON_ERROR_STOP=1 \
            -v schema="$schema" \
            -v seed_block="$seed_block" \
            -v start_block="$start_block" \
            -v end_block="$end_block" \
            < scripts/export-state-parity.sql
        substreams run -e "$endpoint" substreams.yaml map_events \
            -s "$start_block" -t "$stop_block" -o jsonl
    } | target/debug/state-replay
)
replay_status=$?
set -e

printf '%s\n' "$result" | jq
if ((replay_status != 0)); then
    exit "$replay_status"
fi
printf '%s\n' "$result" | jq -e '
    .mismatch_count == 0 and
    .expected_entities == .changed_entities and
    .expected_entities > 0
' >/dev/null
