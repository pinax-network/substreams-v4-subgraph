#!/usr/bin/env bash
set -euo pipefail

if (( $# != 3 )); then
  echo "usage: $0 <graphman-dump-dir> <checkpoint-block> <output-dir>" >&2
  exit 1
fi

dump_dir="$(cd "$1" && pwd)"
checkpoint_block="$2"
output_dir="$3"

for required_command in duckdb jq; do
  command -v "$required_command" >/dev/null 2>&1 || {
    echo "missing required command: $required_command" >&2
    exit 1
  }
done

test ! -e "$output_dir" || { echo "output already exists: $output_dir" >&2; exit 1; }
case "$dump_dir$output_dir" in
  *"'"*) echo "single quotes are not supported in dump/output paths" >&2; exit 1 ;;
esac

mkdir -p "$output_dir/native" "$output_dir/versions" "$output_dir/changes" \
  "$output_dir/snapshots/$checkpoint_block" "$output_dir/clamps"
jq -S . "$dump_dir/metadata.json" > "$output_dir/metadata.json"
cp "$dump_dir/schema.graphql" "$output_dir/schema.graphql"
test ! -f "$dump_dir/subgraph.yaml" || cp "$dump_dir/subgraph.yaml" "$output_dir/subgraph.yaml"
jq -S . "$dump_dir/metadata.json" > "$output_dir/native/metadata.json"
cp "$dump_dir/schema.graphql" "$output_dir/native/schema.graphql"
test ! -f "$dump_dir/subgraph.yaml" || cp "$dump_dir/subgraph.yaml" "$output_dir/native/subgraph.yaml"

while IFS= read -r entity_name; do
  entity_dir="$dump_dir/$entity_name"
  chunk_glob="$entity_dir/chunk_*.parquet"
  mkdir -p "$output_dir/native/$entity_name"
  : > "$output_dir/versions/$entity_name.jsonl"
  : > "$output_dir/changes/$entity_name.jsonl"
  : > "$output_dir/snapshots/$checkpoint_block/$entity_name.jsonl"
  if [[ -d "$entity_dir" ]] && compgen -G "$entity_dir/*.parquet" >/dev/null; then
    cp "$entity_dir"/*.parquet "$output_dir/native/$entity_name/"
  fi
  compgen -G "$chunk_glob" >/dev/null || continue

  duckdb -batch -c "COPY (SELECT * FROM read_parquet('$chunk_glob', union_by_name=true) ORDER BY vid) TO '$output_dir/versions/$entity_name.jsonl' (FORMAT JSON, ARRAY false);"

  columns="$(duckdb -batch -noheader -list -c "SELECT column_name FROM (DESCRIBE SELECT * FROM read_parquet('$chunk_glob', union_by_name=true));")"
  if grep -qx 'block_range_start' <<<"$columns"; then
    predicate="block_range_start <= $checkpoint_block AND (block_range_end IS NULL OR block_range_end > $checkpoint_block)"
    duckdb -batch -c "
      COPY (
        WITH versions AS (
          SELECT *,
                 row_number() OVER (PARTITION BY id ORDER BY block_range_start, vid) AS version_number,
                 lead(block_range_start) OVER (PARTITION BY id ORDER BY block_range_start, vid) AS next_start
          FROM read_parquet('$chunk_glob', union_by_name=true)
        ), changes AS (
          SELECT block_range_start AS block_number,
                 CASE WHEN version_number = 1 THEN 'create' ELSE 'update' END AS operation,
                 vid,
                 id
          FROM versions
          UNION ALL
          SELECT block_range_end AS block_number,
                 'delete' AS operation,
                 vid,
                 id
          FROM versions
          WHERE block_range_end IS NOT NULL
            AND (next_start IS NULL OR next_start <> block_range_end)
        )
        SELECT * FROM changes ORDER BY block_number, operation, vid
      ) TO '$output_dir/changes/$entity_name.jsonl' (FORMAT JSON, ARRAY false);"
  elif grep -Fqx 'block$' <<<"$columns"; then
    predicate="\"block$\" <= $checkpoint_block"
    duckdb -batch -c "
      COPY (
        SELECT \"block$\" AS block_number, 'create' AS operation, vid, id
        FROM read_parquet('$chunk_glob', union_by_name=true)
        ORDER BY block_number, vid
      ) TO '$output_dir/changes/$entity_name.jsonl' (FORMAT JSON, ARRAY false);"
  else
    predicate="true"
  fi
  duckdb -batch -c "COPY (SELECT * FROM read_parquet('$chunk_glob', union_by_name=true) WHERE $predicate ORDER BY vid) TO '$output_dir/snapshots/$checkpoint_block/$entity_name.jsonl' (FORMAT JSON, ARRAY false);"

  clamp_glob="$entity_dir/clamp_*.parquet"
  if compgen -G "$clamp_glob" >/dev/null; then
    duckdb -batch -c "COPY (SELECT * FROM read_parquet('$clamp_glob', union_by_name=true) ORDER BY vid) TO '$output_dir/clamps/$entity_name.jsonl' (FORMAT JSON, ARRAY false);"
  fi
done < <(jq -r '.tables | keys[]' "$dump_dir/metadata.json")
