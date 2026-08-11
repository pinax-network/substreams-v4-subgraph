#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

output=${1:-spkg/uniswap-v4-base-backfill-v0.1.0.spkg}
asset=$(basename "$output")
expected_sha=75b810d18ec1dc78ca5535b2cc56828334c873b93d39500f93ca036499048453
url="https://github.com/pinax-network/substreams-v4-subgraph/releases/download/v0.1.0/$asset"

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

if [[ -f "$output" ]] && [[ "$(sha256_file "$output")" == "$expected_sha" ]]; then
    echo "verified existing $output"
    exit 0
fi

mkdir -p "$(dirname "$output")"
temporary=$(mktemp /tmp/uniswap-v4-base-events.XXXXXX)
trap 'rm -f "$temporary"' EXIT
curl -fsSL "$url" -o "$temporary"
actual_sha=$(sha256_file "$temporary")
if [[ "$actual_sha" != "$expected_sha" ]]; then
    echo "event SPKG SHA-256 mismatch: got $actual_sha, expected $expected_sha" >&2
    exit 1
fi
mv "$temporary" "$output"
echo "downloaded and verified $output"
