#!/usr/bin/env bash
set -euo pipefail

if (( $# != 1 )); then
    echo "usage: $0 <local-restored-schema>" >&2
    exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

: "${KUBECONFIG:?set KUBECONFIG to an authorized read-only cluster config}"
: "${PINAX_API_KEY:?set PINAX_API_KEY for the disposable local oracle}"

local_schema=$1
source_schema=${GRAPH_SCHEMA:-sgd1246}
kube_namespace=${KUBE_NAMESPACE:-subgraphs}
postgres_pod=${POSTGRES_POD:-univ4base-postgres-0}
postgres_user=${POSTGRES_USER:-graph}
postgres_db=${POSTGRES_DB:-graph}
graph_node_workload_kind=${GRAPH_NODE_WORKLOAD_KIND:-deployment}
graph_node_workload=${GRAPH_NODE_WORKLOAD:-graph-node-basegiant-0}
deployment=${DEPLOYMENT_ID:-Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB}
seed_block=${SEED_BLOCK:-26990278}
start_block=${START_BLOCK:-26990279}
end_block=${END_BLOCK:-26990520}

for schema in "$local_schema" "$source_schema"; do
    if [[ ! "$schema" =~ ^sgd[0-9]+$ ]]; then
        echo "schemas must match sgd followed by digits" >&2
        exit 1
    fi
done

replicas=$(kubectl -n "$kube_namespace" get "$graph_node_workload_kind" "$graph_node_workload" \
    -o jsonpath='{.spec.replicas}')
if [[ "$replicas" != "0" ]]; then
    echo "$graph_node_workload_kind/$graph_node_workload must have zero replicas" >&2
    exit 1
fi

local_psql=(docker compose -f oracle/docker-compose.yml exec -T postgres \
    env 'PGOPTIONS=-c default_transaction_read_only=on' \
    psql -U graph -d graph -AtX -v ON_ERROR_STOP=1)
remote_psql=(kubectl -n "$kube_namespace" exec "$postgres_pod" -- \
    env 'PGOPTIONS=-c default_transaction_read_only=on' \
    psql -U "$postgres_user" -d "$postgres_db" -AtX -v ON_ERROR_STOP=1)

actual_local=$("${local_psql[@]}" -c \
    "SELECT subgraph FROM deployment_schemas WHERE name = '$local_schema';")
actual_source=$("${remote_psql[@]}" -c \
    "SELECT subgraph FROM deployment_schemas WHERE name = '$source_schema';")
if [[ "$actual_local" != "$deployment" || "$actual_source" != "$deployment" ]]; then
    echo "local/source schemas must both contain $deployment" >&2
    exit 1
fi

temporary_dir=$(mktemp -d /tmp/substreams-v4-physical-parity.XXXXXX)
trap 'rm -rf "$temporary_dir"' EXIT
results=$temporary_dir/results.jsonl
mismatch_detail=$temporary_dir/first-mismatch.diff
: > "$results"

matched=0
mismatched=0
local_rows_total=0
source_rows_total=0

record_result() {
    local table=$1 status=$2 local_rows=$3 source_rows=$4
    jq -cn \
        --arg table "$table" \
        --arg status "$status" \
        --argjson local_rows "$local_rows" \
        --argjson source_rows "$source_rows" \
        '{table:$table,status:$status,local_rows:$local_rows,source_rows:$source_rows}' \
        >> "$results"
}

compare_queries() {
    local table=$1 local_query=$2 source_query=$3
    local safe_name=${table//\$/_}
    local local_file=$temporary_dir/$safe_name.local.jsonl
    local source_file=$temporary_dir/$safe_name.source.jsonl
    "${local_psql[@]}" -c "$local_query" > "$local_file"
    "${remote_psql[@]}" -c "$source_query" > "$source_file"
    local local_rows source_rows
    local_rows=$(wc -l < "$local_file" | tr -d ' ')
    source_rows=$(wc -l < "$source_file" | tr -d ' ')
    local_rows_total=$((local_rows_total + local_rows))
    source_rows_total=$((source_rows_total + source_rows))
    if cmp -s "$local_file" "$source_file"; then
        matched=$((matched + 1))
        record_result "$table" match "$local_rows" "$source_rows"
    else
        mismatched=$((mismatched + 1))
        record_result "$table" mismatch "$local_rows" "$source_rows"
        if [[ ! -s "$mismatch_detail" ]]; then
            diff -u "$local_file" "$source_file" | sed -n '1,80p' > "$mismatch_detail" || true
        fi
    fi
}

mutable_tables=(
    pool_manager bundle token pool tick uniswap_day_data pool_day_data
    pool_hour_data token_day_data token_hour_data position arrakis_hook 'poi2$'
)
immutable_tables=(transaction swap modify_liquidity subscribe unsubscribe transfer)

for table in "${mutable_tables[@]}"; do
    quoted_table="\"$table\""
    seed_ids=$("${local_psql[@]}" -c \
        "SELECT coalesce(string_agg(quote_literal(id), ',' ORDER BY id), quote_literal('__none__'))
           FROM $local_schema.$quoted_table
          WHERE lower(block_range) < $start_block;")
    canonical="((to_jsonb(t)-'vid'-'block_range') || jsonb_build_object(
        'block_range', int4range(
            lower(block_range),
            CASE WHEN upper(block_range) <= $end_block THEN upper(block_range) ELSE NULL END
        )
    ))::text"
    local_query="SELECT $canonical
                   FROM $local_schema.$quoted_table t
                  ORDER BY lower(block_range), id;"
    source_query="SELECT $canonical
                    FROM $source_schema.$quoted_table t
                   WHERE (block_range @> $seed_block AND id IN ($seed_ids))
                      OR lower(block_range) BETWEEN $start_block AND $end_block
                   ORDER BY lower(block_range), id;"
    compare_queries "$table" "$local_query" "$source_query"
done

for table in "${immutable_tables[@]}"; do
    quoted_table="\"$table\""
    local_query="SELECT (to_jsonb(t)-'vid')::text
                   FROM $local_schema.$quoted_table t
                  ORDER BY \"block$\", id;"
    source_query="SELECT (to_jsonb(t)-'vid')::text
                    FROM $source_schema.$quoted_table t
                   WHERE \"block$\" BETWEEN $start_block AND $end_block
                   ORDER BY \"block$\", id;"
    compare_queries "$table" "$local_query" "$source_query"
done

local_ds=$("${local_psql[@]}" -c "SELECT count(*) FROM $local_schema.\"data_sources$\";")
source_ds=$("${remote_psql[@]}" -c "SELECT count(*) FROM $source_schema.\"data_sources$\";")
if [[ "$local_ds" = "$source_ds" ]]; then
    matched=$((matched + 1))
    record_result 'data_sources$' match "$local_ds" "$source_ds"
else
    mismatched=$((mismatched + 1))
    record_result 'data_sources$' mismatch "$local_ds" "$source_ds"
fi
local_rows_total=$((local_rows_total + local_ds))
source_rows_total=$((source_rows_total + source_ds))

report=$temporary_dir/report.json
jq -s \
    --arg deployment "$deployment" \
    --arg local_schema "$local_schema" \
    --arg source_schema "$source_schema" \
    --argjson seed_block "$seed_block" \
    --argjson start_block "$start_block" \
    --argjson end_block "$end_block" \
    --argjson matched_tables "$matched" \
    --argjson mismatched_tables "$mismatched" \
    --argjson local_rows "$local_rows_total" \
    --argjson source_rows "$source_rows_total" '
    {
      deployment:$deployment,
      local_schema:$local_schema,
      source_schema:$source_schema,
      range:{seed:$seed_block,start:$start_block,end:$end_block},
      vid_policy:"excluded: Graph Node v0.44 restore intentionally allocates new per-table VIDs for specVersion 0.0.4",
      future_range_policy:"upper bounds after the checkpoint are normalized to open ranges",
      matched_tables:$matched_tables,
      mismatched_tables:$mismatched_tables,
      local_rows:$local_rows,
      source_rows:$source_rows,
      tables:.
    }
' "$results" > "$report"

if [[ -n "${PARITY_REPORT:-}" ]]; then
    mkdir -p "$(dirname "$PARITY_REPORT")"
    cp "$report" "$PARITY_REPORT"
    if [[ -s "$mismatch_detail" ]]; then
        cp "$mismatch_detail" "${PARITY_REPORT%.json}.diff"
    fi
fi

jq . "$report"
if (( mismatched > 0 )); then
    if [[ -s "$mismatch_detail" ]]; then
        sed -n '1,80p' "$mismatch_detail" >&2
    fi
    exit 1
fi
