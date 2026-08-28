#!/usr/bin/env bash
set -euo pipefail

if (( $# < 1 || $# > 2 )); then
    echo "usage: $0 <store-state-spkg> [release-contract]" >&2
    exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

package=$1
contract=${2:-packages/base-uniswap-v4-v0.5.0.json}
[[ -f "$package" ]] || {
    echo "missing Store-state package: $package" >&2
    exit 1
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

expected_sha=$(jq -er '.packages.store_state.sha256' "$contract")
actual_sha=$(sha256_file "$package")
if [[ "$actual_sha" != "$expected_sha" ]]; then
    echo "Store-state SPKG SHA-256 mismatch: got $actual_sha, expected $expected_sha" >&2
    exit 1
fi

info=$(mktemp /tmp/store-state-package-info.XXXXXX)
trap 'rm -f "$info"' EXIT
module=$(jq -er '.packages.store_state.module' "$contract")
substreams info "$package" "$module" --json >"$info"

jq -e --slurpfile contract "$contract" '
    .name == $contract[0].packages.store_state.package_name and
    .version == $contract[0].packages.store_state.package_version and
    .network == $contract[0].network and
    ([.modules[] | select(.name == "store_tick_liquidity")] | length) == 0 and
    ([.modules[] | select(.name | startswith("store_tick_liquidity_"))] | length) ==
      $contract[0].packages.store_state.tick_liquidity_shards.count
' "$info" >/dev/null

while IFS=$'\t' read -r module_name expected; do
    actual=$(jq -er --arg name "$module_name" \
        '.modules[] | select(.name == $name) | .hash' "$info")
    if [[ "$actual" != "$expected" ]]; then
        echo "$module_name hash mismatch: got $actual, expected $expected" >&2
        exit 1
    fi
done < <(jq -r '
    (.packages.store_state.imported_module_hashes +
     (.packages.store_state.partition_module_hashes // {}) +
     .packages.store_state.tick_liquidity_shards.module_hashes +
     {(.packages.store_state.module): .packages.store_state.module_hash}) |
    to_entries[] | [.key, .value] | @tsv
' "$contract")

expected_delta_inputs=$(jq -c \
    '.packages.store_state.tick_liquidity_shards.module_hashes | keys' "$contract")
partition_count=$(jq -er '.packages.store_state.partition_module_hashes // {} | length' "$contract")
if (( partition_count == 0 )); then
    actual_delta_inputs=$(jq -c --arg module_name "$module" '
        [.modules[] | select(.name == $module_name) | .inputs[] |
         select(.type == "store" and .mode == "deltas") | .name]
    ' "$info")
else
    expected_partitions=$(jq -c '.packages.store_state.partition_module_hashes | keys' "$contract")
    actual_partitions=$(jq -c --arg module_name "$module" '
        [.modules[] | select(.name == $module_name) | .inputs[] |
         select(.type == "map" and (.name | startswith("map_store_state_inputs_"))) | .name]
    ' "$info")
    if [[ "$actual_partitions" != "$expected_partitions" ]]; then
        echo "Store-state assembler partitions differ from the release contract" >&2
        exit 1
    fi
    actual_delta_inputs=$(jq -c --slurpfile contract "$contract" '
        [.modules[] |
         select(.name as $name |
           $contract[0].packages.store_state.partition_module_hashes | has($name)) |
         .inputs[] | select(.type == "store" and .mode == "deltas") | .name]
    ' "$info")
fi
if [[ "$actual_delta_inputs" != "$expected_delta_inputs" ]]; then
    echo "Store-state partition Tick shard inputs differ from the release contract" >&2
    exit 1
fi

if (( partition_count > 0 )); then
    jq -e --slurpfile contract "$contract" '
        [.modules[] |
         select(.name as $name |
           $contract[0].packages.store_state.partition_module_hashes | has($name)) |
         (.inputs | length == 22) and
         (.output_type == "proto:pinax.uniswap.v4.base.store.v1.ReducerInputs")] |
        all
    ' "$info" >/dev/null
fi

while IFS= read -r shard; do
    jq -e --arg shard "$shard" '
        .modules[] | select(.name == $shard) |
        .kind == "store" and
        .value_type == "bigint" and
        .update_policy == "add" and
        .inputs == [{type:"map",name:"state:events:map_events"}]
    ' "$info" >/dev/null
done < <(jq -r '.packages.store_state.tick_liquidity_shards.module_hashes | keys[]' "$contract")

shard_count=$(jq -er '.packages.store_state.tick_liquidity_shards.count' "$contract")
echo "verified Linux Store-state SPKG hash, module graph, and $shard_count Tick shard hashes"
