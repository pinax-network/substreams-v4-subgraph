#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

for required_command in curl jq; do
  command -v "$required_command" >/dev/null 2>&1 || {
    echo "missing required command: $required_command" >&2
    exit 1
  }
done

while IFS= read -r lock_file; do
  artifact_root="$(dirname "$lock_file")"
  while IFS=$'\t' read -r relative_file expected_cid; do
    artifact_file="$artifact_root/$relative_file"
    actual_cid="$(
      curl -fsS -X POST \
        -F "file=@$artifact_file" \
        "http://127.0.0.1:15001/api/v0/add?cid-version=0&pin=true&quiet=true" \
        | tail -n 1 \
        | jq -r .Hash
    )"
    test "$actual_cid" = "$expected_cid" || {
      echo "IPFS import mismatch for $artifact_file: $actual_cid" >&2
      exit 1
    }
  done < <(jq -r '.artifacts[] | [.path, .cid] | @tsv' "$lock_file")
done < <(find "$repository_root/artifacts/deployment" -name artifacts.lock.json -type f | sort)

echo "all pinned deployment objects are present in the local IPFS node"
