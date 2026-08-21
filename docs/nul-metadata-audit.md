# Exhaustive NUL metadata audit

The production backfill uses compatibility profile
`postgres-nul-truncate-v1` after Base block `34,546,212`. The profile changes
only two `Token` entities and only their `name` and `symbol` fields: raw ERC-20
metadata is truncated at the first `U+0000` before Graph Node entity and POI
reduction.

`substreams-nul-audit.yaml` is the independent, exhaustive source-side audit
for that exception. It imports the immutable v0.4.0 Store-state package and
adds one downstream map, `map_nul_metadata_audit`. The map emits a record only
when an Initialize event reads token `name` or `symbol` metadata containing a
NUL byte. It records:

- canonical block number and hash;
- transaction hash and Firehose log ordinal;
- token address and field name;
- the complete raw UTF-8 byte sequence;
- the value produced by truncating at the first NUL.

Clean blocks encode to zero bytes. The certification runtime therefore uses
`substreams-native --include-empty-frames` over the fixed inclusive range
`[34,546,213, 49,477,581]`. The resulting 12-byte frame headers prove that
every block was observed even though only exception blocks carry protobuf
payloads. The evidence collector rejects any gap, duplicate, trailing frame,
malformed payload, unexpected token/field/value, incorrect first exception,
or range shorter than the fixed target.

## Cache boundary

The audit WASM is isolated from every imported module. Changing it changes
only `map_nul_metadata_audit`; every imported event, Store, shard, assembler,
and `map_store_state_inputs` hash remains byte-identical to release v0.4.0.

Verify that contract with:

```bash
make nul-audit-package
./scripts/test-nul-audit-cache-boundary.sh
./scripts/verify-nul-audit-package.sh \
  downloads/uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg
```

Host-built SPKG bytes are behavioral and cache-boundary evidence only. The
canonical package used by terminal certification must be the immutable GitHub
release asset built by CI, and consumers must pin both its SHA-256 and the
`map_nul_metadata_audit` module hash.

## Bounded production scan

After downloading the released SPKG and setting `SUBSTREAMS_API_TOKEN`, stream
the exhaustive framed input with the native transport shipped by the matching
certification runtime:

```bash
substreams-native \
  -e base-substreams-tier1-prod.kan-sst2.pinax.io:443 \
  uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg \
  map_nul_metadata_audit \
  -s 34546213 -t 49477582 \
  -H X-Substreams-Parallel-Workers:500 \
  --limit-processed-blocks 50000000 \
  --include-empty-frames > nul-metadata-audit.framed
```

The stop block is exclusive. Terminal certification retains the framed-stream
SHA-256, released SPKG SHA-256, audit module hash, complete frame inventory,
and the exact four unique correction tuples in its primary difference report.
