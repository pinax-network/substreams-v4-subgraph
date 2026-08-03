#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

: "${KUBECONFIG:?set KUBECONFIG to an authorized read-only cluster config}"
: "${SUBSTREAMS_API_TOKEN:?set SUBSTREAMS_API_TOKEN for the Base endpoint}"

fixture=${PARITY_RANGES:-fixtures/base-ranges.json}
if [[ ! -f "$fixture" ]]; then
    echo "parity range fixture does not exist: $fixture" >&2
    exit 1
fi

temporary_dir=$(mktemp -d /tmp/substreams-v4-logical-parity.XXXXXX)
trap 'rm -rf "$temporary_dir"' EXIT
results=$temporary_dir/results.jsonl
: > "$results"

cargo build --locked --target wasm32-unknown-unknown --release
cargo build --locked --features native --bin state-replay

graft_seed=$(jq -er '.graft_seed.block' "$fixture")
while IFS= read -r range; do
    name=$(jq -er '.name' <<< "$range")
    start_block=$(jq -er '.start_block' <<< "$range")
    end_block=$(jq -er '.end_block' <<< "$range")
    if [[ "$name" == "first-child-core-events" ]]; then
        seed_block=$graft_seed
    else
        seed_block=$((start_block - 1))
    fi

    echo "verifying $name ($start_block..$end_block, seed $seed_block)" >&2
    result=$(
        SKIP_BUILD=1 \
        SEED_BLOCK="$seed_block" \
        START_BLOCK="$start_block" \
        END_BLOCK="$end_block" \
        ./scripts/verify-state-parity.sh
    )
    jq -cn \
        --arg name "$name" \
        --argjson seed_block "$seed_block" \
        --argjson start_block "$start_block" \
        --argjson end_block "$end_block" \
        --argjson result "$result" \
        '{name:$name,range:{seed:$seed_block,start:$start_block,end:$end_block},result:$result}' \
        >> "$results"
done < <(jq -c '.ranges[]' "$fixture")

report=$temporary_dir/report.json
jq -s \
    --arg deployment "$(jq -er '.deployment' "$fixture")" '
    {
      deployment:$deployment,
      ranges:length,
      processed_blocks:(map(.range.end - .range.start + 1) | add),
      expected_entities:(map(.result.expected_entities) | add),
      changed_entities:(map(.result.changed_entities) | add),
      poi_ranges:(map(select(.result.poi_checked == true)) | length),
      mismatch_count:(map(.result.mismatch_count) | add),
      results:.
    }
' "$results" > "$report"

if [[ -n "${PARITY_REPORT:-}" ]]; then
    mkdir -p "$(dirname "$PARITY_REPORT")"
    cp "$report" "$PARITY_REPORT"
fi

jq . "$report"
jq -e '
    .ranges > 0 and
    .mismatch_count == 0 and
    .poi_ranges == .ranges and
    .expected_entities == .changed_entities and
    all(.results[]; .result.expected_entities > 0)
' "$report" >/dev/null
