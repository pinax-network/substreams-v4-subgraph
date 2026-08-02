#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
substreams protogen substreams.yaml

if [[ -n "$(git status --porcelain --untracked-files=all -- src/pb)" ]]; then
  echo "generated protobuf bindings differ; run 'make protogen' and commit src/pb" >&2
  git status --short -- src/pb >&2
  exit 1
fi

echo "generated protobuf bindings are current"
