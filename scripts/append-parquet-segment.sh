#!/usr/bin/env bash
set -euo pipefail

if (( $# != 5 )); then
    echo "usage: $0 <dump-dir> <input-checkpoint> <start-block> <end-block> <new-checkpoint>" >&2
    exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

: "${SUBSTREAMS_API_TOKEN:?set SUBSTREAMS_API_TOKEN for the Base endpoint}"

dump_dir=$(cd "$1" && pwd)
input_checkpoint=$2
start_block=$3
end_block=$4
new_checkpoint=$5
endpoint=${ENDPOINT:-base-substreams-tier1-prod.kan-sst2.pinax.io:443}
rpc_url=${BASE_RPC_URL:-https://mainnet.base.org}

if [[ -e "$new_checkpoint" ]]; then
    echo "checkpoint already exists: $new_checkpoint" >&2
    exit 1
fi
current_head=$(jq -er '.head_block.number' "$dump_dir/metadata.json")
if (( start_block != current_head + 1 || end_block < start_block )); then
    echo "segment must start at $((current_head + 1)) and have a non-empty range" >&2
    exit 1
fi
head_tag=$(printf '0x%x' "$end_block")
head_hash=$(curl -fsS -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getBlockByNumber\",\"params\":[\"$head_tag\",false]}" \
    "$rpc_url" | jq -er '.result.hash')

work_dir=$(mktemp -d /tmp/substreams-v4-segment.XXXXXX)
trap 'rm -rf "$work_dir"' EXIT
segment_snapshot=$work_dir/segment.json

cargo build --locked --features native --bin state-replay --bin parquet-backfill
substreams run -e "$endpoint" substreams.yaml map_events \
    -s "$start_block" -t "$((end_block + 1))" -o jsonl \
    | target/debug/state-replay \
        --input-snapshot "$input_checkpoint" \
        --snapshot "$segment_snapshot" >/dev/null
target/debug/parquet-backfill \
    --snapshot "$segment_snapshot" \
    --output "$dump_dir" \
    --head-block "$end_block" \
    --head-hash "$head_hash" \
    --mode append \
    --checkpoint "$new_checkpoint"
