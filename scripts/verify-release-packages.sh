#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

dist=${1:-dist}
contract=packages/base-uniswap-v4-v0.4.0.json

for command in jq substreams; do
    command -v "$command" >/dev/null 2>&1 || {
        echo "missing required command: $command" >&2
        exit 1
    }
done

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

for key in state_stores store_fed store_state; do
    asset=$(jq -er --arg key "$key" '.packages[$key].asset' "$contract")
    expected_sha=$(jq -er --arg key "$key" '.packages[$key].sha256' "$contract")
    package="$dist/$asset"
    [[ -f "$package" ]] || {
        echo "missing release package: $package" >&2
        exit 1
    }
    actual_sha=$(sha256_file "$package")
    [[ "$actual_sha" == "$expected_sha" ]] || {
        echo "$asset SHA-256 mismatch: got $actual_sha, expected $expected_sha" >&2
        exit 1
    }
    printf '%s  %s\n' "$actual_sha" "$asset" >"$package.sha256"

    module=$(jq -er --arg key "$key" '.packages[$key].module' "$contract")
    info=$(mktemp /tmp/uniswap-v4-package-info.XXXXXX)
    substreams info "$package" "$module" --json >"$info"
    jq -e --arg key "$key" --slurpfile contract "$contract" '
        .name == $contract[0].packages[$key].package_name and
        .version == $contract[0].packages[$key].package_version and
        (.modules[] | select(.name == $contract[0].packages[$key].module) |
          .hash == $contract[0].packages[$key].module_hash and
          .output_type == $contract[0].packages[$key].output_type)
    ' "$info" >/dev/null
    rm -f "$info"
done

state_info=$(mktemp /tmp/uniswap-v4-state-package.XXXXXX)
store_info=$(mktemp /tmp/uniswap-v4-store-state-package.XXXXXX)
trap 'rm -f "$state_info" "$store_info"' EXIT
substreams info "$dist/$(jq -r '.packages.state_stores.asset' "$contract")" --json >"$state_info"
substreams info "$dist/$(jq -r '.packages.store_state.asset' "$contract")" --json >"$store_info"

while IFS=$'\t' read -r module expected; do
    actual=$(jq -er --arg name "$module" '.modules[] | select(.name == $name) | .hash' "$state_info")
    [[ "$actual" == "$expected" ]] || {
        echo "$module hash mismatch: got $actual, expected $expected" >&2
        exit 1
    }
done < <(jq -r '.packages.state_stores.store_module_hashes | to_entries[] | [.key,.value] | @tsv' "$contract")

expected_shards=$(jq -r '.packages.store_state.tick_liquidity_shards.count' "$contract")
actual_shards=$(jq '[.modules[] | select(.name | startswith("store_tick_liquidity_"))] | length' "$store_info")
[[ "$actual_shards" == "$expected_shards" ]] || {
    echo "Store-state shard count mismatch: got $actual_shards, expected $expected_shards" >&2
    exit 1
}

while IFS=$'\t' read -r module expected; do
    actual=$(jq -er --arg name "$module" '.modules[] | select(.name == $name) | .hash' "$store_info")
    [[ "$actual" == "$expected" ]] || {
        echo "$module hash mismatch: got $actual, expected $expected" >&2
        exit 1
    }
done < <(jq -r '.packages.store_state.tick_liquidity_shards.module_hashes | to_entries[] | [.key,.value] | @tsv' "$contract")

echo "verified byte-identical v0.4.0 Store release packages and module hashes"
