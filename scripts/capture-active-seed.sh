#!/usr/bin/env bash
set -euo pipefail

if (( $# != 2 )); then
    echo "usage: $0 <native-graft-seed-dump> <new-snapshot-file>" >&2
    exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

: "${KUBECONFIG:?set KUBECONFIG to an authorized read-only cluster config}"

seed_dump=$(cd "$1" && pwd)
snapshot=$2
if [[ -e "$snapshot" ]]; then
    echo "snapshot already exists: $snapshot" >&2
    exit 1
fi

metadata=$seed_dump/metadata.json
deployment=${DEPLOYMENT_ID:-Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB}
schema=${GRAPH_SCHEMA:-sgd1246}
kube_namespace=${KUBE_NAMESPACE:-subgraphs}
postgres_pod=${POSTGRES_POD:-univ4base-postgres-0}
postgres_user=${POSTGRES_USER:-graph}
postgres_db=${POSTGRES_DB:-graph}
graph_node_workload_kind=${GRAPH_NODE_WORKLOAD_KIND:-deployment}
graph_node_workload=${GRAPH_NODE_WORKLOAD:-graph-node-basegiant-0}
seed_block=$(jq -er '.head_block.number' "$metadata")

if [[ ! "$schema" =~ ^sgd[0-9]+$ ]]; then
    echo "GRAPH_SCHEMA must match sgd followed by digits" >&2
    exit 1
fi
jq -e \
    --arg deployment "$deployment" '
    .version == 1 and
    .deployment == $deployment and
    .network == "base" and
    .head_block.number == 26990278 and
    .graft_block.number == 26990278 and
    (.tables | length) == 20
' "$metadata" >/dev/null
cmp -s "$seed_dump/schema.graphql" "artifacts/deployment/$deployment/schema.graphql"
cmp -s "$seed_dump/subgraph.yaml" "artifacts/deployment/$deployment/subgraph.yaml"

replicas=$(kubectl -n "$kube_namespace" get "$graph_node_workload_kind" "$graph_node_workload" \
    -o jsonpath='{.spec.replicas}')
if [[ "$replicas" != "0" ]]; then
    echo "$graph_node_workload_kind/$graph_node_workload must have zero replicas" >&2
    exit 1
fi
actual_deployment=$(kubectl -n "$kube_namespace" exec "$postgres_pod" -- \
    psql -U "$postgres_user" -d "$postgres_db" -AtX -v ON_ERROR_STOP=1 \
    -c "SET default_transaction_read_only=on; SELECT subgraph FROM deployment_schemas WHERE name = '$schema';")
actual_deployment=${actual_deployment##*$'\n'}
if [[ "$actual_deployment" != "$deployment" ]]; then
    echo "$schema contains '$actual_deployment', expected '$deployment'" >&2
    exit 1
fi

work_dir=$(mktemp -d /tmp/substreams-v4-active-seed.XXXXXX)
trap 'rm -rf "$work_dir"' EXIT
export_file=$work_dir/active-seed.jsonl
complete=0
for attempt in 1 2 3; do
    : > "$export_file"
    if kubectl -n "$kube_namespace" exec -i "$postgres_pod" -- \
        psql -U "$postgres_user" -d "$postgres_db" -AtX -v ON_ERROR_STOP=1 \
        -v schema="$schema" -v seed_block="$seed_block" \
        < scripts/export-active-seed.sql > "$export_file" && \
        jq -Rse \
            --argjson seed_block "$seed_block" '
            split("\n") | map(fromjson? | select(. != null)) as $rows |
            ($rows | map(select(has("@active_seed_complete"))) | last."@active_seed_complete") as $manifest |
            $manifest.seed_block == $seed_block and
            ([$rows[] | select(has("@seed_version"))] | length) == $manifest.seed_records and
            ([$rows[] | select(has("@poi_seed"))] | length) == $manifest.poi_records
        ' "$export_file" >/dev/null; then
        complete=1
        break
    fi
    echo "active seed export attempt $attempt was incomplete" >&2
done
if (( complete == 0 )); then
    echo "active seed export failed its completeness manifest" >&2
    exit 1
fi

table_file=$work_dir/tables.jsonl
jq -c '
    .tables | to_entries[] |
    select(.key != "data_sources$") |
    {"@table":{entity_type:.key,max_vid:.value.max_vid}}
' "$metadata" > "$table_file"

cargo build --locked --features native --bin state-replay
{ cat "$table_file"; cat "$export_file"; } \
    | target/debug/state-replay --snapshot "$snapshot" >/dev/null

expected_seed=$(jq -Rsr '
    split("\n") | map(fromjson? | select(has("@active_seed_complete"))) |
    last."@active_seed_complete".seed_records
' "$export_file")
jq -e \
    --argjson expected_seed "$expected_seed" '
    (.seed_versions | length) == $expected_seed and
    (.table_max_vids | length) == 19 and
    .poi_seed != null and
    (.state.changes | length) == 0
' "$snapshot" >/dev/null

jq -n \
    --arg snapshot "$snapshot" \
    --argjson seed_block "$seed_block" \
    --argjson active_rows "$expected_seed" '
    {snapshot:$snapshot,seed_block:$seed_block,active_rows:$active_rows,status:"complete"}
'
