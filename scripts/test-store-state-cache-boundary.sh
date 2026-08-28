#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

probe_wasm=store-probe/target/wasm32-unknown-unknown/release/uniswap_v4_state_store_probe.wasm
tick_wasm=store-tick-shards/target/wasm32-unknown-unknown/release/uniswap_v4_store_tick_shards.wasm
[[ -f "$probe_wasm" ]] || {
    echo "missing probe WASM: run make stores-build first" >&2
    exit 1
}
[[ -f "$tick_wasm" ]] || {
    echo "missing Tick shard WASM: run make store-state-build first" >&2
    exit 1
}

temporary_probe=$(mktemp /tmp/store-state-boundary-probe.XXXXXX)
temporary_tick=$(mktemp /tmp/store-state-boundary-tick.XXXXXX)
temporary_manifest=$(mktemp "$repo_dir/.store-state-boundary.XXXXXX")
mv "$temporary_manifest" "$temporary_manifest.yaml"
temporary_manifest=$temporary_manifest.yaml
temporary_tick_manifest=$(mktemp "$repo_dir/.store-state-tick-boundary.XXXXXX")
mv "$temporary_tick_manifest" "$temporary_tick_manifest.yaml"
temporary_tick_manifest=$temporary_tick_manifest.yaml
baseline=$(mktemp /tmp/store-state-baseline.XXXXXX)
modified=$(mktemp /tmp/store-state-modified.XXXXXX)
tick_modified=$(mktemp /tmp/store-state-tick-modified.XXXXXX)
state_package=$(mktemp /tmp/store-state-import.XXXXXX)
trap 'rm -f "$temporary_probe" "$temporary_manifest" "$temporary_tick" "$temporary_tick_manifest" "$baseline" "$modified" "$tick_modified" "$state_package"' EXIT

cp "$probe_wasm" "$temporary_probe"
# A valid custom section changes only the downstream assembler binary.
printf '\000\016\014cache-marker\001' >>"$temporary_probe"

sed "s|file: ./store-probe/target/wasm32-unknown-unknown/release/uniswap_v4_state_store_probe.wasm|file: $temporary_probe|" \
    substreams-store-state.yaml >"$temporary_manifest"

substreams info substreams-store-state.yaml --json >"$baseline"
substreams info "$temporary_manifest" --json >"$modified"
substreams info downloads/uniswap-v4-base-state-stores-v0.1.0.spkg --json >"$state_package"

while IFS= read -r module; do
    before=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$baseline")
    after=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$modified")
    if [[ "$before" != "$after" ]]; then
        echo "$module hash changed when only the assembler binary changed" >&2
        exit 1
    fi
done < <(jq -r '.modules[] | select(.kind == "store") | .name' "$baseline")

for module in store_pool_tick store_pool_transaction_count store_pool_liquidity; do
    expected=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$state_package")
    actual=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$baseline")
    if [[ "$actual" != "$expected" ]]; then
        echo "$module Store-state import hash mismatch: got $actual, expected $expected" >&2
        exit 1
    fi
done

for shard in $(seq 0 63); do
    module=$(printf 'store_tick_liquidity_%02d' "$shard")
    jq -er --arg module_name "$module" \
        '.modules[] | select(.name == $module_name) | .hash | test("^[0-9a-f]{40}$")' \
        "$baseline" >/dev/null
done

for module in \
    store_pool_sqrt_price \
    store_token_decimals \
    store_pool_token_decimals \
    map_store_state_inputs; do
    jq -er --arg module_name "$module" \
        '.modules[] | select(.name == $module_name) | .hash | test("^[0-9a-f]{40}$")' \
        "$baseline" >/dev/null
done

before=$(jq -er '.modules[] | select(.name == "map_store_state_inputs") | .hash' "$baseline")
after=$(jq -er '.modules[] | select(.name == "map_store_state_inputs") | .hash' "$modified")
if [[ "$before" == "$after" ]]; then
    echo "map_store_state_inputs hash did not change with its binary" >&2
    exit 1
fi

cp "$tick_wasm" "$temporary_tick"
printf '\000\016\014cache-marker\002' >>"$temporary_tick"
sed "s|file: ./store-tick-shards/target/wasm32-unknown-unknown/release/uniswap_v4_store_tick_shards.wasm|file: $temporary_tick|" \
    substreams-store-state.yaml >"$temporary_tick_manifest"
substreams info "$temporary_tick_manifest" --json >"$tick_modified"

while IFS= read -r module; do
    before=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$baseline")
    after=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$tick_modified")
    if [[ "$module" == store_tick_liquidity_* ]]; then
        if [[ "$before" == "$after" ]]; then
            echo "$module hash did not change with the Tick shard binary" >&2
            exit 1
        fi
    elif [[ "$before" != "$after" ]]; then
        echo "$module hash changed with an unrelated Tick shard binary" >&2
        exit 1
    fi
done < <(jq -r '.modules[] | select(.kind == "store") | .name' "$baseline")

map_before=$(jq -er '.modules[] | select(.name == "map_store_state_inputs") | .hash' "$baseline")
map_after=$(jq -er '.modules[] | select(.name == "map_store_state_inputs") | .hash' "$tick_modified")
if [[ "$map_before" == "$map_after" ]]; then
    echo "map_store_state_inputs hash did not follow the Tick shard input graph" >&2
    exit 1
fi

echo "verified three imported Store hashes, 64 isolated Tick shards, and downstream cache boundaries"
