#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

probe_wasm=store-probe/target/wasm32-unknown-unknown/release/uniswap_v4_state_store_probe.wasm
[[ -f "$probe_wasm" ]] || {
    echo "missing probe WASM: run make stores-build first" >&2
    exit 1
}

temporary_probe=$(mktemp /tmp/store-cache-boundary-probe.XXXXXX)
temporary_manifest=$(mktemp "$repo_dir/.store-cache-boundary.XXXXXX")
mv "$temporary_manifest" "$temporary_manifest.yaml"
temporary_manifest=$temporary_manifest.yaml
baseline=$(mktemp /tmp/store-cache-baseline.XXXXXX)
modified=$(mktemp /tmp/store-cache-modified.XXXXXX)
state_package=$(mktemp /tmp/store-cache-import.XXXXXX)
trap 'rm -f "$temporary_probe" "$temporary_manifest" "$baseline" "$modified" "$state_package"' EXIT

cp "$probe_wasm" "$temporary_probe"
# Append a valid WASM custom section named "cache-marker". It changes only the
# downstream map binary and leaves every Store writer byte-for-byte untouched.
printf '\000\016\014cache-marker\001' >> "$temporary_probe"

sed "s|file: ./store-probe/target/wasm32-unknown-unknown/release/uniswap_v4_state_store_probe.wasm|file: $temporary_probe|" \
    substreams-store-fed.yaml > "$temporary_manifest"

substreams info substreams-store-fed.yaml --json > "$baseline"
substreams info "$temporary_manifest" --json > "$modified"
substreams info downloads/uniswap-v4-base-state-stores-v0.1.0.spkg --json > "$state_package"

for module in \
    store_pool_tick \
    store_pool_transaction_count \
    store_tick_liquidity \
    store_pool_liquidity; do
    before=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$baseline")
    after=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$modified")
    if [[ "$before" != "$after" ]]; then
        echo "$module hash changed when only the probe binary changed" >&2
        exit 1
    fi
done

for module in \
    store_pool_tick \
    store_pool_transaction_count \
    store_tick_liquidity \
    store_pool_liquidity; do
    expected=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$state_package")
    actual=$(jq -er --arg module_name "$module" '.modules[] | select(.name == $module_name) | .hash' "$baseline")
    if [[ "$actual" != "$expected" ]]; then
        echo "$module import hash mismatch: got $actual, expected $expected" >&2
        exit 1
    fi
done

decoder_expected=$(jq -er '.modules[] | select(.name == "events:map_events") | .hash' "$state_package")
decoder_actual=$(jq -er '.modules[] | select(.name == "state:events:map_events") | .hash' "$baseline")
if [[ "$decoder_actual" != "$decoder_expected" ]]; then
    echo "imported decoder hash mismatch: got $decoder_actual, expected $decoder_expected" >&2
    exit 1
fi

before=$(jq -er '.modules[] | select(.name == "map_reducer_inputs") | .hash' "$baseline")
after=$(jq -er '.modules[] | select(.name == "map_reducer_inputs") | .hash' "$modified")
if [[ "$before" == "$after" ]]; then
    echo "map_reducer_inputs hash did not change with its binary" >&2
    exit 1
fi

echo "verified imported Store hashes and downstream-only reducer-input changes"
