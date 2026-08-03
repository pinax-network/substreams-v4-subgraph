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
page_size=${ACTIVE_SEED_PAGE_SIZE:-2000}
if [[ ! "$page_size" =~ ^[1-9][0-9]*$ ]] || (( page_size > 10000 )); then
    echo "ACTIVE_SEED_PAGE_SIZE must be between 1 and 10000" >&2
    exit 1
fi

seed_stream=$work_dir/active-seed.jsonl
poi_file=$work_dir/poi.jsonl
: > "$seed_stream"

poi_complete=0
for attempt in 1 2 3; do
    : > "$poi_file"
    if kubectl -n "$kube_namespace" exec -i "$postgres_pod" -- \
        psql -U "$postgres_user" -d "$postgres_db" -AtX -v ON_ERROR_STOP=1 \
        -v schema="$schema" -v seed_block="$seed_block" \
        < scripts/export-active-poi-seed.sql > "$poi_file" && \
        jq -Rse --argjson seed_block "$seed_block" '
            split("\n") | map(fromjson? | select(. != null)) as $rows |
            ($rows | map(select(has("@active_poi_seed_complete"))) |
                last."@active_poi_seed_complete") as $manifest |
            $manifest.seed_block == $seed_block and
            ([$rows[] | select(has("@poi_seed"))] | length) == $manifest.records and
            $manifest.records == 1
        ' "$poi_file" >/dev/null; then
        poi_complete=1
        break
    fi
    echo "active POI seed export attempt $attempt was incomplete" >&2
done
if (( poi_complete == 0 )); then
    echo "active POI seed export failed its completeness manifest" >&2
    exit 1
fi

tables=(
    'PoolManager:pool_manager'
    'Bundle:bundle'
    'Token:token'
    'Pool:pool'
    'Tick:tick'
    'UniswapDayData:uniswap_day_data'
    'PoolDayData:pool_day_data'
    'PoolHourData:pool_hour_data'
    'TokenDayData:token_day_data'
    'TokenHourData:token_hour_data'
    'Position:position'
    'ArrakisHook:arrakis_hook'
)
active_rows=0
page_count=0
for entry in "${tables[@]}"; do
    entity_type=${entry%%:*}
    table=${entry#*:}
    after_vid=-1
    while true; do
        page_file=$work_dir/page.jsonl
        page_complete=0
        for attempt in 1 2 3; do
            : > "$page_file"
            if kubectl -n "$kube_namespace" exec -i "$postgres_pod" -- \
                psql -U "$postgres_user" -d "$postgres_db" -AtX -v ON_ERROR_STOP=1 \
                -v schema="$schema" -v table="$table" -v entity_type="$entity_type" \
                -v seed_block="$seed_block" -v after_vid="$after_vid" \
                -v page_size="$page_size" \
                < scripts/export-active-seed-page.sql > "$page_file"; then
                page_info=$(jq -Rsr \
                    --arg entity_type "$entity_type" \
                    --argjson seed_block "$seed_block" \
                    --argjson after_vid "$after_vid" '
                    split("\n") | map(fromjson? | select(. != null)) as $rows |
                    ($rows | map(select(has("@active_seed_page_complete"))) |
                        last."@active_seed_page_complete") as $manifest |
                    if $manifest.entity_type == $entity_type and
                       $manifest.seed_block == $seed_block and
                       $manifest.after_vid == $after_vid and
                       ([$rows[] | select(has("@seed_version"))] | length) == $manifest.records
                    then [$manifest.records, $manifest.last_vid] | @tsv
                    else empty
                    end
                ' "$page_file")
                if [[ -n "$page_info" ]]; then
                    page_complete=1
                    break
                fi
            fi
            echo "$entity_type page after VID $after_vid attempt $attempt was incomplete" >&2
        done
        if (( page_complete == 0 )); then
            echo "$entity_type page after VID $after_vid failed its completeness manifest" >&2
            exit 1
        fi
        IFS=$'\t' read -r records last_vid <<< "$page_info"
        if (( records > page_size || (records > 0 && last_vid <= after_vid) )); then
            echo "$entity_type page did not advance monotonically" >&2
            exit 1
        fi
        # Preserve PostgreSQL's exact arbitrary-precision numeric lexemes. jq
        # would reserialize large integer JSON numbers in exponent notation.
        awk 'index($0, "{\"@seed_version\":") == 1 { print }' \
            "$page_file" >> "$seed_stream"
        active_rows=$((active_rows + records))
        page_count=$((page_count + 1))
        if (( records < page_size )); then
            break
        fi
        after_vid=$last_vid
    done
done

table_file=$work_dir/tables.jsonl
jq -c '
    .tables | to_entries[] |
    select(.key != "data_sources$") |
    {"@table":{entity_type:.key,max_vid:.value.max_vid}}
' "$metadata" > "$table_file"

cargo build --locked --features native --bin state-replay
temporary_snapshot=$snapshot.pending
{ cat "$table_file"; cat "$poi_file"; cat "$seed_stream"; } \
    | target/debug/state-replay --snapshot "$temporary_snapshot" --quiet
mv "$temporary_snapshot" "$snapshot"

jq -e \
    --argjson expected_seed "$active_rows" '
    (.seed_versions | length) == $expected_seed and
    (.table_max_vids | length) == 19 and
    .poi_seed != null and
    (.state.changes | length) == 0
' "$snapshot" >/dev/null

jq -n \
    --arg snapshot "$snapshot" \
    --argjson seed_block "$seed_block" \
    --argjson active_rows "$active_rows" \
    --argjson pages "$page_count" '
    {snapshot:$snapshot,seed_block:$seed_block,active_rows:$active_rows,pages:$pages,status:"complete"}
'
