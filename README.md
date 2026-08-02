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
- Module parameters reject any network, deployment, graft, address, or start
  block that differs from the pinned deployment.
- Canonical Base receipt fixtures cover all seven event kinds, malformed logs,
  signed integer boundaries, and multiple relevant events per transaction.

Native Graph Node Parquet production remains a separate layer so physical dump
format concerns cannot change trigger extraction or entity semantics.

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
Graph Node StatefulSet has any replicas:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export SUBSTREAMS_API_TOKEN=...
make verify-state-parity
```

The default oracle is `sgd1246` in `univ4base-postgres-0`, with the
`graph-node-basegiant-0` Deployment paused. Override the Kubernetes, Postgres,
schema, range, workload, or endpoint settings through the environment. The
command verifies the exact deployment ID before reading, seeds state at block
26,990,278, replays child blocks 26,990,279 through 26,990,520, and rejects both
field mismatches and unexpected entity writes.

The committed receipt fixtures make tests network-independent. Refresh them
only when intentionally re-verifying the canonical Base blocks:

```bash
./scripts/capture-event-fixtures.mjs
```

## Reference material

- [deployed artifact inventory](docs/deployed-artifacts.md)
- [logical and physical parity contract](docs/parity-contract.md)
- [state reducer architecture and validation](docs/state-reducer.md)
- [pinned Base fixture ranges](fixtures/base-ranges.json)
- [Graph Node reference oracle](oracle/README.md)

Artifact and live canonical-range checks remain available independently:

```bash
./scripts/verify-pinned-artifacts.sh
./scripts/verify-base-fixtures.mjs
```
