#!/usr/bin/env bash
set -euo pipefail

mode="${1:-root}"
oracle_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch_dir="$(mktemp -d)"
first_output="$scratch_dir/first"
second_output="$scratch_dir/second"
trap 'rm -rf "$scratch_dir"' EXIT

"$oracle_root/export-fixture.sh" "$mode" "$first_output"
"$oracle_root/export-fixture.sh" "$mode" "$second_output"
diff -ru "$first_output" "$second_output"

echo "$mode oracle exports are byte-identical after canonicalization"
