#!/usr/bin/env bash
set -euo pipefail

if (( $# != 1 )); then
    echo "usage: $0 <uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg>" >&2
    exit 1
fi

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

package=$1
test -f "$package" && test ! -L "$package" || {
    echo "NUL audit package is missing or is a symlink: $package" >&2
    exit 1
}

work=$(mktemp -d /tmp/verify-nul-audit-package.XXXXXX)
trap 'rm -rf "$work"' EXIT
substreams info "$package" --json > "$work/audit.json"
substreams info release/v0.4.0/uniswap-v4-base-store-state-reducer-v0.4.0.spkg \
    --json > "$work/imported.json"

jq -e '
    .name == "uniswap_v4_base_nul_metadata_audit" and
    .version == "v0.1.0" and
    .network == "base" and
    ([.modules[] | select(.name == "map_nul_metadata_audit")] | length) == 1 and
    any(.modules[];
      .name == "map_nul_metadata_audit" and .initial_block == 25350988 and
      .kind == "map" and
      .inputs == [{type:"map",name:"state:map_store_state_inputs"}] and
      .output_type == "proto:pinax.uniswap.v4.base.audit.v1.NulMetadataAudit" and
      (.hash | test("^[0-9a-f]{40}$")))
' "$work/audit.json" >/dev/null || {
    echo "NUL audit package identity or output contract is invalid" >&2
    exit 1
}
imported_count=$(jq -er '.modules | length' "$work/imported.json")
audit_count=$(jq -er '.modules | length' "$work/audit.json")
test "$audit_count" -eq "$((imported_count + 1))" || {
    echo "NUL audit package contains unexpected additional modules" >&2
    exit 1
}

while IFS= read -r module; do
    expected=$(jq -er --arg name "$module" \
        '.modules[] | select(.name == $name) | .hash' "$work/imported.json")
    actual=$(jq -er --arg name "state:$module" \
        '.modules[] | select(.name == $name) | .hash' "$work/audit.json")
    test "$actual" = "$expected" || {
        echo "released import hash changed for module $module" >&2
        exit 1
    }
done < <(jq -r '.modules[].name' "$work/imported.json")

if command -v sha256sum >/dev/null 2>&1; then
    package_sha=$(sha256sum "$package" | awk '{print $1}')
else
    package_sha=$(shasum -a 256 "$package" | awk '{print $1}')
fi
module_hash=$(jq -er '.modules[] | select(.name == "map_nul_metadata_audit") | .hash' \
    "$work/audit.json")
jq -n --arg package_sha256 "$package_sha" --arg module_hash "$module_hash" \
    '{status:"verified",package:"uniswap_v4_base_nul_metadata_audit",
      version:"v0.1.0",module:"map_nul_metadata_audit",
      module_hash:$module_hash,package_sha256:$package_sha256,
      imported_v0_4_0_module_hashes_preserved:true}'
