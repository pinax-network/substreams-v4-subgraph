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
full_seed=${FULL_SEED:-0}
seed_id_scope=${SEED_ID_SCOPE:-all}
page_size=${PHYSICAL_PARITY_PAGE_SIZE:-5000}
page_retries=${PHYSICAL_PARITY_RETRIES:-5}

for schema in "$local_schema" "$source_schema"; do
    if [[ ! "$schema" =~ ^sgd[0-9]+$ ]]; then
        echo "schemas must match sgd followed by digits" >&2
        exit 1
    fi
done
if [[ "$full_seed" != "0" && "$full_seed" != "1" ]]; then
    echo "FULL_SEED must be 0 or 1" >&2
    exit 1
fi
if [[ "$seed_id_scope" != "all" && "$seed_id_scope" != "changed" ]]; then
    echo "SEED_ID_SCOPE must be all or changed" >&2
    exit 1
fi
if [[ ! "$page_size" =~ ^[1-9][0-9]*$ ]] || (( page_size > 10000 )); then
    echo "PHYSICAL_PARITY_PAGE_SIZE must be between 1 and 10000" >&2
    exit 1
fi
if [[ ! "$page_retries" =~ ^[1-9][0-9]*$ ]] || (( page_retries > 10 )); then
    echo "PHYSICAL_PARITY_RETRIES must be between 1 and 10" >&2
    exit 1
fi

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
    local table=$1 local_query=$2 source_query=$3 seed_ids_file=${4:-}
    local safe_name=${table//\$/_}
    local local_file=$temporary_dir/$safe_name.local.jsonl
    local source_file=$temporary_dir/$safe_name.source.jsonl
    if [[ -n "$seed_ids_file" && "$seed_id_scope" = "changed" ]]; then
        {
            printf 'WITH compare_seed_ids(id) AS (SELECT id FROM (VALUES (NULL::text)'
            while IFS= read -r id; do
                printf ',(%s)' "$id"
            done < "$seed_ids_file"
            printf ') AS ids(id) WHERE id IS NOT NULL)\n'
            printf '%s;\n' "$local_query"
        } | "${local_psql[@]}" > "$local_file"
    else
        "${local_psql[@]}" -c "$local_query" > "$local_file"
    fi
    LC_ALL=C sort -o "$local_file" "$local_file"
    : > "$source_file"
    local after_vid=-1
    while true; do
        local page_file=$temporary_dir/$safe_name.source-page.jsonl
        local page_compressed=$temporary_dir/$safe_name.source-page.jsonl.gz
        local page_complete=0
        for attempt in $(seq 1 "$page_retries"); do
            : > "$page_compressed"
            if {
                if [[ -n "$seed_ids_file" ]]; then
                    printf 'WITH compare_seed_ids(id) AS (SELECT id FROM (VALUES (NULL::text)'
                    while IFS= read -r id; do
                        printf ',(%s)' "$id"
                    done < "$seed_ids_file"
                    printf ') AS ids(id) WHERE id IS NOT NULL), parity_rows AS (\n'
                else
                    printf 'WITH parity_rows AS (\n'
                fi
                printf '%s\n' "$source_query"
                printf ') SELECT vid::text || E\x27\\t\x27 || canonical_row FROM parity_rows WHERE vid > %s ORDER BY vid LIMIT %s;\n' \
                    "$after_vid" "$page_size"
            } | kubectl -n "$kube_namespace" exec -i "$postgres_pod" -- \
                bash -o pipefail -c '
                    env "PGOPTIONS=-c default_transaction_read_only=on" \
                        psql -U "$1" -d "$2" -qAtX -v ON_ERROR_STOP=1 | gzip -1
                ' bash "$postgres_user" "$postgres_db" > "$page_compressed" && \
                gzip -t "$page_compressed" && \
                gzip -dc "$page_compressed" > "$page_file"; then
                page_complete=1
                break
            fi
            echo "$table parity page after VID $after_vid attempt $attempt was incomplete" >&2
            sleep "$attempt"
        done
        if (( page_complete == 0 )); then
            echo "$table parity page after VID $after_vid failed integrity checks" >&2
            exit 1
        fi
        local page_rows
        page_rows=$(wc -l < "$page_file" | tr -d ' ')
        if (( page_rows == 0 )); then
            break
        fi
        local last_vid
        last_vid=$(tail -n 1 "$page_file" | cut -f 1)
        if [[ ! "$last_vid" =~ ^[0-9]+$ ]] || (( last_vid <= after_vid || page_rows > page_size )); then
            echo "$table parity page did not advance monotonically" >&2
            exit 1
        fi
        cut -f 2- "$page_file" >> "$source_file"
        if (( page_rows < page_size )); then
            break
        fi
        after_vid=$last_vid
    done
    LC_ALL=C sort -o "$source_file" "$source_file"
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
    seed_ids_file=""
    if [[ "$full_seed" = "0" ]]; then
        safe_name=${table//\$/_}
        seed_ids_file=$temporary_dir/$safe_name.seed-ids
        if [[ "$seed_id_scope" = "changed" ]]; then
            seed_id_predicate="lower(block_range) BETWEEN $start_block AND $end_block"
        else
            seed_id_predicate="block_range @> $seed_block"
        fi
        "${local_psql[@]}" -c \
            "SELECT DISTINCT quote_literal(id)
               FROM $local_schema.$quoted_table
              WHERE $seed_id_predicate
              ORDER BY quote_literal(id);" > "$seed_ids_file"
    fi
    canonical="((to_jsonb(t)-'vid'-'block_range') || jsonb_build_object(
        'block_range', int4range(
            lower(block_range),
            CASE WHEN upper(block_range) <= $end_block THEN upper(block_range) ELSE NULL END
        )
    ))::text"
    if [[ "$full_seed" = "0" && "$seed_id_scope" = "changed" ]]; then
        local_seed_predicate="block_range @> $seed_block AND id IN (SELECT id FROM compare_seed_ids)"
    else
        local_seed_predicate="block_range @> $seed_block"
    fi
    local_query="SELECT $canonical
                   FROM $local_schema.$quoted_table t
                  WHERE ($local_seed_predicate)
                     OR lower(block_range) BETWEEN $start_block AND $end_block"
    if [[ "$full_seed" = "1" ]]; then
        source_query="SELECT vid, $canonical AS canonical_row
                        FROM $source_schema.$quoted_table t
                       WHERE block_range @> $seed_block
                          OR lower(block_range) BETWEEN $start_block AND $end_block"
    else
        source_query="SELECT vid, $canonical AS canonical_row
                        FROM $source_schema.$quoted_table t
                       WHERE (block_range @> $seed_block AND id IN (SELECT id FROM compare_seed_ids))
                          OR lower(block_range) BETWEEN $start_block AND $end_block"
    fi
    compare_queries "$table" "$local_query" "$source_query" "$seed_ids_file"
done

for table in "${immutable_tables[@]}"; do
    quoted_table="\"$table\""
    local_query="SELECT (to_jsonb(t)-'vid')::text
                   FROM $local_schema.$quoted_table t
                  WHERE \"block$\" BETWEEN $start_block AND $end_block;"
    source_query="SELECT vid, (to_jsonb(t)-'vid')::text AS canonical_row
                    FROM $source_schema.$quoted_table t
                   WHERE \"block$\" BETWEEN $start_block AND $end_block"
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
    --argjson source_rows "$source_rows_total" \
    --arg seed_id_scope "$seed_id_scope" \
    --argjson full_seed "$full_seed" '
    {
      deployment:$deployment,
      local_schema:$local_schema,
      source_schema:$source_schema,
      range:{seed:$seed_block,start:$start_block,end:$end_block},
      vid_policy:"excluded: Graph Node v0.44 restore renumbers legacy dump rows; restored sequences are audited separately",
      future_range_policy:"upper bounds after the checkpoint are normalized to open ranges",
      seed_policy:(
        if $full_seed == 1 then "complete active seed"
        elif $seed_id_scope == "changed" then "IDs changed in the comparison range"
        else "all local IDs active at the seed"
        end
      ),
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
