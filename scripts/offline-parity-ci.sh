#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_dir"

if (( $# != 1 )); then
    echo "usage: $0 <artifact-directory>" >&2
    exit 1
fi

artifact_dir=$1
mkdir -p "$artifact_dir"
log=$artifact_dir/offline-parity.log
report=$artifact_dir/offline-parity.json

set +e
cargo test --locked --all-features -- --nocapture 2>&1 | tee "$log"
test_status=${PIPESTATUS[0]}
set -e

if (( test_status == 0 )); then
    status=pass
else
    status=fail
fi

jq -n \
    --arg status "$status" \
    --argjson exit_code "$test_status" '
    {
      format_version:1,
      suite:"offline-parity",
      status:$status,
      exit_code:$exit_code,
      contracts:[
        "all seven canonical Base event kinds decode from pinned receipts",
        "malformed and wrong-address logs are ignored",
        "signed boundaries and Graph Node trigger order are preserved",
        "all 18 schema entities reduce deterministically",
        "Graph Node save order and decimal behavior are exact"
      ],
      log:"offline-parity.log"
    }
' > "$report"

exit "$test_status"
