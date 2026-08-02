#!/usr/bin/env bash
set -euo pipefail

if (( $# != 1 )); then
    echo "usage: $0 <new-dump-directory>" >&2
    exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

output=$1
if [[ -e "$output" ]]; then
    echo "output path already exists: $output" >&2
    exit 1
fi

head_block=$(jq -r '.child.end_block' fixtures/oracle-ranges.json)
head_hash=$(jq -r '.child.end_hash' fixtures/oracle-ranges.json)
snapshot=$(mktemp /tmp/substreams-v4-snapshot.XXXXXX)
rm -f "$snapshot"
trap 'rm -f "$snapshot"' EXIT

SNAPSHOT_OUTPUT="$snapshot" ./scripts/verify-state-parity.sh
cargo build --locked --features native --bin parquet-backfill
target/debug/parquet-backfill \
    --snapshot "$snapshot" \
    --output "$output" \
    --head-block "$head_block" \
    --head-hash "$head_hash"

jq -e --argjson head_block "$head_block" --arg head_hash "${head_hash#0x}" '
    .version == 1
    and .deployment == "Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB"
    and .head_block.number == $head_block
    and .head_block.hash == $head_hash
    and .graft_base == "QmS1ehFzXTD9eA1f1EgjZvdyAj2EHtVNMrEN91H3pLuHMy"
    and .graft_block.number == 26990278
    and (.tables | length) == 20
    and (.tables["Poi$"].chunks | length) == 2
' "$output/metadata.json" >/dev/null

echo "Graph Node-native fixture written to $output"
