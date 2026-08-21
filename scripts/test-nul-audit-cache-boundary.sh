#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

audit_wasm=nul-audit/target/wasm32-unknown-unknown/release/uniswap_v4_base_nul_audit.wasm
test -f "$audit_wasm" || {
    echo "missing NUL audit WASM: run make stores-build first" >&2
    exit 1
}

baseline=$(mktemp /tmp/nul-audit-baseline.XXXXXX)
imported=$(mktemp /tmp/nul-audit-imported.XXXXXX)
modified=$(mktemp /tmp/nul-audit-modified.XXXXXX)
temporary_wasm=$(mktemp /tmp/nul-audit-wasm.XXXXXX)
temporary_manifest=$(mktemp "$repo_dir/.nul-audit-boundary.XXXXXX")
mv "$temporary_manifest" "$temporary_manifest.yaml"
temporary_manifest=$temporary_manifest.yaml
trap 'rm -f "$baseline" "$imported" "$modified" "$temporary_wasm" "$temporary_manifest"' EXIT

substreams info substreams-nul-audit.yaml --json > "$baseline"
substreams info release/v0.4.0/uniswap-v4-base-store-state-reducer-v0.4.0.spkg \
    --json > "$imported"

while IFS= read -r module; do
    expected=$(jq -er --arg name "$module" \
        '.modules[] | select(.name == $name) | .hash' "$imported")
    actual=$(jq -er --arg name "state:$module" \
        '.modules[] | select(.name == $name) | .hash' "$baseline")
    test "$actual" = "$expected" || {
        echo "imported module hash changed in NUL audit package: $module" >&2
        exit 1
    }
done < <(jq -r '.modules[].name' "$imported")

cp "$audit_wasm" "$temporary_wasm"
printf '\000\016\014audit-marker\001' >> "$temporary_wasm"
sed "s|file: ./nul-audit/target/wasm32-unknown-unknown/release/uniswap_v4_base_nul_audit.wasm|file: $temporary_wasm|" \
    substreams-nul-audit.yaml > "$temporary_manifest"
substreams info "$temporary_manifest" --json > "$modified"

while IFS= read -r module; do
    before=$(jq -er --arg name "$module" \
        '.modules[] | select(.name == $name) | .hash' "$baseline")
    after=$(jq -er --arg name "$module" \
        '.modules[] | select(.name == $name) | .hash' "$modified")
    if [[ "$module" = map_nul_metadata_audit ]]; then
        test "$before" != "$after" || {
            echo "audit map hash did not change with its isolated WASM" >&2
            exit 1
        }
    else
        test "$before" = "$after" || {
            echo "imported module hash changed with audit-only WASM" >&2
            exit 1
        }
    fi
done < <(jq -r '.modules[].name' "$baseline")

echo "verified NUL audit imports every v0.4.0 module byte-for-byte"
