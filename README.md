# Substreams Uniswap v4 Base backfill

This repository is a proof of concept for replacing the historical Graph Node
WASM backfill of Uniswap v4 on Base with parallel Substreams computation, then
restoring Graph Node v0.44-native Parquet into the existing deployment.

The only success target is deployment
`Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB` in its original Graph Node
database. A separate database, alternate deployment ID, generic Uniswap event
stream, or alternate query service is not equivalent.

## Implemented foundation

- Both deployed IPFS artifact sets and their checksums are pinned under
  `artifacts/deployment/`.
- `oracle/` provides the digest-pinned Graph Node v0.44 reference environment,
  native Graphman dump, canonical export, GraphQL, and POI workflow.
- The Rust `map_events` module decodes only the exact PoolManager,
  PositionManager, and ArrakisHookFactory data sources and handler events.
- Every decoded event carries block, transaction origin, transaction/log index,
  effective gas price, Firehose ordinal, and Graph Node's block-global log-index
  trigger order.
- Historical ERC-20 calls are batched into the parallel module and apply the
  deployed mapping's string/bytes32, unknown, zero, and `<255` fallbacks.
- The deterministic Rust reducer implements all 18 schema entities, exact
  handler save order, graft checkpoint seeding, Uniswap liquidity math, and
  Graph Node v0.44's pinned 34-significant-digit decimal behavior.
- The native sink emits Graph Node v0.44 dump-compatible Arrow 58.3/Parquet
  58.3, temporal versions and clamps, table-local VIDs, metadata, graft
  pointers, and the deployment's exact legacy `Poi$` stream.
- Resumable generation carries forward the generated active VIDs and POI seed,
  appends atomic chunks/clamps to a native graft dump, and journals completed
  tables with SHA-256 verification.
- Module parameters reject any network, deployment, graft, address, or start
  block that differs from the pinned deployment.
- Canonical Base receipt fixtures cover all seven event kinds, malformed logs,
  signed integer boundaries, and multiple relevant events per transaction.

Parquet production remains isolated from trigger extraction and entity
semantics so physical dump concerns cannot change the deterministic reducer.

## Prototype status

Both prototype gates are certified end to end on the original deployment
identity. The logical gate has zero entity or POI differences at every pinned
checkpoint. The 100,000-block physical gate restored 1,155,270 rows through
unmodified Graph Node v0.44 and matched all 20 PostgreSQL tables. Graph Node
then continued for 1,000 requested blocks, matched all 10,366 changed-range
rows, survived restart, and rewound byte-identically to the Parquet seed.
Replacing that advanced disposable copy with the preserved checkpoint also
returned byte-identically with 20/20 valid table sequences.

The measured cached historical pipeline runs at 203.0 blocks/s before the
one-time restore, and repeated restored Graph Node continuations reached
27.8–31.25 requested blocks/s, both comfortably above Base's sampled 0.5 blocks/s arrival
rate. See the [100k benchmark](docs/benchmark-100k.md) for stage timings,
resource use, storage projections, and the production-hardening decision.

No production replacement was performed. Live `sgd1246` remains outside this
prototype's mutation scope until the separately approved backup and cutover
gates in the runbook are satisfied.

## Build and validate

The repository pins Rust 1.90.0 and all direct Rust dependencies. From a clean
checkout:

```bash
make validate
```

That regenerates and checks protobuf bindings, runs offline tests, applies
formatting and Clippy gates, builds the WASM, and writes
`spkg/uniswap-v4-base-backfill-v0.1.0.spkg`.

Run the default bounded first-child fixture range against the Base Substreams
endpoint (the stop block is exclusive):

```bash
export SUBSTREAMS_API_TOKEN=...
make run
```

Override `ENDPOINT`, `START_BLOCK`, or `STOP_BLOCK` as needed. The defaults are
`base-substreams-tier1-prod.kan-sst2.pinax.io:443` and
`26990279:26990521`.

The first post-graft entity differential uses only targeted, read-only versions
from a paused Graph Node database copy. It refuses to run while that copy's
Graph Node writer workload has any replicas:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export SUBSTREAMS_API_TOKEN=...
make verify-state-parity
```

The default oracle is `sgd1246` in `univ4base-postgres-0`, with the
`graph-node-basegiant-0` Deployment paused. Override the Kubernetes, Postgres,
schema, range, workload, or endpoint settings through the environment. The
command verifies the exact deployment ID before reading, seeds state at block
26,990,278, replays child blocks 26,990,279 through 26,990,520, and rejects
field mismatches, unexpected entity writes, write-order differences, and POI
differences. Run all four pinned logical ranges with
`make verify-all-state-ranges`.

Build and restore the bounded Graph Node-native Parquet proof fixture with the
commands in [Graph Node-native Parquet](docs/native-parquet.md). The fixture is
selective and is not a production replacement dump.

Exercise a two-segment append, including an optional planned interruption:

```bash
STOP_AFTER_TABLES=7 make build-resumable-parquet-fixture \
  OUTPUT=/tmp/uniswap-v4-resumable-dump
```

After restoring into the disposable Graph Node v0.44 oracle, compare every
restored table with the paused source copy:

```bash
export PINAX_API_KEY="$SUBSTREAMS_API_TOKEN"
make verify-restored-parity LOCAL_SCHEMA=sgd3
```

Then prove Graph Node continuation, restart persistence, forced rewind, and
deterministic replay against the same original deployment identity:

```bash
make verify-restored-lifecycle \
  LOCAL_SCHEMA=sgd7 \
  NAME=oracle/native-parquet-resume-script-1
```

The committed receipt fixtures make tests network-independent. Refresh them
only when intentionally re-verifying the canonical Base blocks:

```bash
./scripts/capture-event-fixtures.mjs
```

## Reference material

- [deployed artifact inventory](docs/deployed-artifacts.md)
- [logical and physical parity contract](docs/parity-contract.md)
- [state reducer architecture and validation](docs/state-reducer.md)
- [Graph Node-native Parquet build and restore](docs/native-parquet.md)
- [differential validation evidence and commands](docs/differential-validation.md)
- [complete-history segmented backfill procedure](docs/production-backfill.md)
- [100k-block Base giant benchmark and decision](docs/benchmark-100k.md)
- [pinned Base fixture ranges](fixtures/base-ranges.json)
- [Graph Node reference oracle](oracle/README.md)

Artifact and live canonical-range checks remain available independently:

```bash
./scripts/verify-pinned-artifacts.sh
./scripts/verify-base-fixtures.mjs
```
