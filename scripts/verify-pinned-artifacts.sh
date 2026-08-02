#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
deployment_id="Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB"
artifact_root="$repository_root/artifacts/deployment/$deployment_id"
lock_file="$artifact_root/artifacts.lock.json"

for required_command in jq ipfs; do
  if ! command -v "$required_command" >/dev/null 2>&1; then
    echo "missing required command: $required_command" >&2
    exit 1
  fi
done

if command -v sha256sum >/dev/null 2>&1; then
  sha256_file() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  sha256_file() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  echo "missing required command: sha256sum or shasum" >&2
  exit 1
fi

file_bytes() {
  if stat -f %z "$1" >/dev/null 2>&1; then
    stat -f %z "$1"
  else
    stat -c %s "$1"
  fi
}

jq -e '.format_version == 1' "$lock_file" >/dev/null
jq -e --arg deployment "$deployment_id" '.deployment.id == $deployment' "$lock_file" >/dev/null

while IFS=$'\t' read -r relative_file expected_cid expected_sha expected_bytes; do
  artifact_file="$artifact_root/$relative_file"
  test -f "$artifact_file" || { echo "missing artifact: $relative_file" >&2; exit 1; }

  actual_sha="$(sha256_file "$artifact_file")"
  test "$actual_sha" = "$expected_sha" || {
    echo "sha256 mismatch for $relative_file: $actual_sha" >&2
    exit 1
  }

  actual_bytes="$(file_bytes "$artifact_file")"
  test "$actual_bytes" = "$expected_bytes" || {
    echo "size mismatch for $relative_file: $actual_bytes" >&2
    exit 1
  }

  actual_cid="$(ipfs add --only-hash --cid-version=0 -Q "$artifact_file")"
  test "$actual_cid" = "$expected_cid" || {
    echo "CID mismatch for $relative_file: $actual_cid" >&2
    exit 1
  }
done < <(jq -r '.artifacts[] | [.path, .cid, .sha256, (.bytes | tostring)] | @tsv' "$lock_file")

manifest_cids="$(mktemp)"
locked_references="$(mktemp)"
actual_entities="$(mktemp)"
locked_entities="$(mktemp)"
trap 'rm -f "$manifest_cids" "$locked_references" "$actual_entities" "$locked_entities"' EXIT

grep -Eo 'Qm[1-9A-HJ-NP-Za-km-z]{44}' "$artifact_root/subgraph.yaml" \
  | sort -u > "$manifest_cids"
jq -r '([.artifacts[] | select(.role != "manifest") | .cid] + [.external_cids[].cid])[]' "$lock_file" \
  | sort -u > "$locked_references"
diff -u "$locked_references" "$manifest_cids"

sed -nE 's/^type[[:space:]]+([^[:space:]]+)[[:space:]]+@entity.*/\1/p' "$artifact_root/schema.graphql" > "$actual_entities"
jq -r '.schema.entities[]' "$lock_file" > "$locked_entities"
diff -u "$locked_entities" "$actual_entities"
test "$(wc -l < "$actual_entities" | tr -d ' ')" = "$(jq -r '.schema.entity_count' "$lock_file")"

for wasm_file in "$artifact_root"/mappings/*.wasm; do
  test "$(od -An -tx1 -N4 "$wasm_file" | tr -d ' \n')" = "0061736d" || {
    echo "invalid WASM header: $wasm_file" >&2
    exit 1
  }
done

jq -e '.format_version == 1 and (.ranges | length == 4)' "$repository_root/fixtures/base-ranges.json" >/dev/null

echo "verified 11 IPFS artifacts, 18 entities, 3 WASM modules, and 4 fixture ranges"
