#!/usr/bin/env bash
set -euo pipefail

if (( $# != 1 )); then
    echo "usage: $0 <new-dump-directory>" >&2
    exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

: "${KUBECONFIG:?set KUBECONFIG to an authorized read-only cluster config}"
: "${SUBSTREAMS_API_TOKEN:?set SUBSTREAMS_API_TOKEN for the Base endpoint}"

output=$1
if [[ -e "$output" ]]; then
    echo "output path already exists: $output" >&2
    exit 1
fi

fixture=fixtures/resume-range.json
endpoint=${ENDPOINT:-base-substreams-tier1-prod.kan-sst2.pinax.io:443}
seed_block=$(jq -er '.seed_block' "$fixture")
first_start=$(jq -er '.segments[0].start_block' "$fixture")
first_end=$(jq -er '.segments[0].end_block' "$fixture")
first_hash=$(jq -er '.segments[0].end_hash' "$fixture")
second_start=$(jq -er '.segments[1].start_block' "$fixture")
second_end=$(jq -er '.segments[1].end_block' "$fixture")
second_hash=$(jq -er '.segments[1].end_hash' "$fixture")

if (( first_start != seed_block + 1 || second_start != first_end + 1 )); then
    echo "resume fixture segments are not contiguous" >&2
    exit 1
fi

work_dir=$(mktemp -d /tmp/substreams-v4-resume.XXXXXX)
trap 'rm -rf "$work_dir"' EXIT
first_snapshot=$work_dir/segment-1.json
second_oracle=$work_dir/segment-2-oracle.json
first_supplemented=$work_dir/segment-1-supplemented.json
first_checkpoint=$work_dir/segment-1-checkpoint.json
second_resumed=$work_dir/segment-2-resumed.json
final_checkpoint=$work_dir/final-checkpoint.json

SEED_BLOCK="$seed_block" START_BLOCK="$first_start" END_BLOCK="$first_end" \
    SNAPSHOT_OUTPUT="$first_snapshot" ./scripts/verify-state-parity.sh >/dev/null
SEED_BLOCK="$first_end" START_BLOCK="$second_start" END_BLOCK="$second_end" \
    SNAPSHOT_OUTPUT="$second_oracle" ./scripts/verify-state-parity.sh >/dev/null

cargo build --locked --features native --bin state-replay --bin parquet-backfill
target/debug/state-replay \
    --input-snapshot "$first_snapshot" \
    --supplement-seeds "$second_oracle" \
    --snapshot "$first_supplemented" \
    --quiet </dev/null
target/debug/parquet-backfill \
    --snapshot "$first_supplemented" \
    --output "$output" \
    --head-block "$first_end" \
    --head-hash "$first_hash" \
    --mode write \
    --checkpoint "$first_checkpoint" >/dev/null

substreams run -e "$endpoint" substreams.yaml map_events \
    -s "$second_start" -t "$((second_end + 1))" -o jsonl \
    | target/debug/state-replay \
        --input-snapshot "$first_checkpoint" \
        --snapshot "$second_resumed" \
        --quiet

planned_stop=false
if [[ -n "${STOP_AFTER_TABLES:-}" ]]; then
    planned_stop=true
    set +e
    target/debug/parquet-backfill \
        --snapshot "$second_resumed" \
        --output "$output" \
        --head-block "$second_end" \
        --head-hash "$second_hash" \
        --mode append \
        --stop-after-tables "$STOP_AFTER_TABLES" \
        >$work_dir/planned-stop.log 2>&1
    append_status=$?
    set -e
    if (( append_status == 0 )) || [[ ! -f "$output/.substreams-append.json" ]]; then
        echo "planned append stop did not leave a resumable journal" >&2
        exit 1
    fi
    jq -e --argjson completed "$STOP_AFTER_TABLES" '
        (.tables | length) == $completed and
        all(.tables[];
            ((.chunk == null and .chunk_sha256 == null) or
             (.chunk != null and (.chunk_sha256 | test("^[0-9a-f]{64}$")))) and
            ((.clamp == null and .clamp_sha256 == null) or
             (.clamp != null and (.clamp_sha256 | test("^[0-9a-f]{64}$"))))
        )
    ' "$output/.substreams-append.json" >/dev/null
fi

target/debug/parquet-backfill \
    --snapshot "$second_resumed" \
    --output "$output" \
    --head-block "$second_end" \
    --head-hash "$second_hash" \
    --mode append \
    --checkpoint "$final_checkpoint" >/dev/null

test ! -e "$output/.substreams-append.json"
jq -e \
    --arg deployment "$(jq -er '.deployment' "$fixture")" \
    --argjson head "$second_end" \
    --arg hash "${second_hash#0x}" '
    .version == 1 and
    .deployment == $deployment and
    .head_block.number == $head and
    .head_block.hash == $hash and
    (.tables | length) == 20
' "$output/metadata.json" >/dev/null
jq -e \
    --argjson head "$second_end" '
    (.state.changes | length) == 0 and
    (.state.processed_blocks | length) == 0 and
    .poi_seed.block_range_start == $head
' "$final_checkpoint" >/dev/null

jq -n \
    --arg output "$output" \
    --argjson seed_block "$seed_block" \
    --argjson first_end "$first_end" \
    --argjson head_block "$second_end" \
    --argjson planned_stop "$planned_stop" '
    {
      output:$output,
      seed_block:$seed_block,
      segments:[{end:$first_end},{end:$head_block}],
      planned_stop_recovered:$planned_stop,
      status:"complete"
    }
'
