# Uniswap v4 Base Substreams

This repository builds and publishes the complete checksum-pinned Substreams
pipeline for the Base Uniswap v4 deployment
`Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB`.

It owns deterministic event extraction, cacheable Store modules, compact
Store-fed output maps, protobuf contracts, manifests, and SPKG releases. The
native Graph Node entity reducer, proof-of-indexing implementation, PostgreSQL
COPY/Parquet writers, lifecycle controls, and physical parity harness live in
the internal
[`pinax-network/substreams-graph-node-backfill`](https://github.com/pinax-network/substreams-graph-node-backfill)
repository.

## Deployment contract

| Input | Value |
| --- | --- |
| Network | Base |
| Output module | `map_events` |
| Output protobuf | `proto:pinax.uniswap.v4.base.v1.Events` |
| Module initial block | `25,350,988` inclusive |
| PoolManager start | `25,350,988` |
| PositionManager start | `25,350,993` |
| ArrakisHookFactory start | `28,450,225` |
| Graft block | `26,990,278` inclusive |
| First child block | `26,990,279` |

Module parameters reject a network, deployment, graft, address, or start block
that differs from the pinned manifest.

## What `map_events` guarantees

- decodes only the deployed PoolManager, PositionManager, and
  ArrakisHookFactory addresses;
- covers Initialize, ModifyLiquidity, Swap, Subscription, Unsubscription,
  Transfer, and LogCreatePrivateHook;
- preserves block and transaction references, origin, effective gas price,
  transaction/log indexes, Firehose ordinal, and Graph Node trigger order;
- batches historical ERC-20 metadata calls;
- matches the deployed mapping's string/bytes32, unknown, zero, and `<255`
  metadata fallbacks;
- ignores malformed logs and logs from unpinned addresses;
- remains independently verifiable from committed canonical Base fixtures.

`map_events` is the raw event contract. Production backfill consumers use the
Store-owned `map_store_state_inputs` output and the matching Graph Node
backfill runtime release.

## Store-owned production path

The package graph and its cache boundary are documented in
[`docs/store-packages.md`](docs/store-packages.md). The production package adds:

- immutable Pool tick, liquidity, and transaction-count Stores;
- 16 size-bounded Tick-liquidity Store shards;
- Pool square-root-price and token-decimal Stores;
- cached exact Graph Decimal computations;
- `map_store_state_inputs`, which emits compact protobuf frames for the native
  Graph Node reducer.

All Substreams state handling stays here. Graph Node entity save order,
temporal rows, POI, VIDs, clamps, checkpoints, and database transactions remain
owned by the backfill runtime.

## Compatibility audit package

`substreams-nul-audit.yaml` imports the immutable v0.4.0 Store-state package
and adds a downstream-only `map_nul_metadata_audit` module. It proves the
complete scope of the production `postgres-nul-truncate-v1` exception over the
fixed backfill range without changing any cached Store or reducer-input module
hash.

The package emits only NUL-bearing token metadata occurrences. Certification
uses zero-length frames for clean blocks, so the retained stream proves full
block coverage while remaining compact. See
[`docs/nul-metadata-audit.md`](docs/nul-metadata-audit.md) for the package,
range, cache, evidence, and release contracts.

## Build and validate

The repository pins Rust 1.90.0 and all direct dependencies:

```bash
make validate
```

This checks generated protobuf bindings, runs extraction and Store tests,
applies formatting and strict Clippy, builds all WASM modules, validates the
three manifests and their cache boundaries, and writes:

```text
spkg/uniswap-v4-base-backfill-v0.1.1.spkg
```

Run the bounded first-child range:

```bash
export SUBSTREAMS_API_TOKEN=...
make run
```

Defaults:

- endpoint: `base-substreams-tier1-prod.kan-sst2.pinax.io:443`;
- range: `[26,990,279, 26,990,521)`.

Override `ENDPOINT`, `START_BLOCK`, or `STOP_BLOCK` when testing another pinned
range.

## Release

Release `v0.1.0` contains:

- `uniswap-v4-base-backfill-v0.1.0.spkg`;
- its SHA-256 checksum;
- module hash
  `8fbf7c14cefe3d5b0c249a8cf4566bcaa8d2ed26`;
- package SHA-256
  `75b810d18ec1dc78ca5535b2cc56828334c873b93d39500f93ca036499048453`.

The backfill repository pins those values in
`adapters/uniswap-v4-base/compatibility.json` and refuses a mismatched package.
Package `v0.1.1` keeps the same `map_events` contract while moving the native
Graph Node runtime and its documentation out of this repository.

Release `v0.4.0` contains the three Store packages used by the production
backfill path:

- `uniswap-v4-base-state-stores-v0.1.0.spkg`;
- `uniswap-v4-base-store-fed-reducer-v0.1.0.spkg`;
- `uniswap-v4-base-store-state-reducer-v0.4.0.spkg`.

The assets are byte-identical to the packages certified by the backfill
runtime before repository ownership was corrected. GitHub exposes the digest
of each release asset; their authoritative compatibility hashes and module
graph are also recorded in
[`packages/base-uniswap-v4-v0.4.0.json`](packages/base-uniswap-v4-v0.4.0.json).
Stage and verify the immutable release artifacts with:

```bash
make release-packages
./scripts/verify-release-packages.sh dist
```

Release `v0.4.1` adds the evidence-only package
`uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg`. CI builds and publishes that
asset from this repository; it imports the released v0.4.0 module graph so all
existing Store caches remain reusable. No checksum sidecar is published.

## Cached production range

The production `sink noop` cache build completed successfully for
`[25,350,988, 49,477,582)` using the released package and 500 parallel workers.
The stop block is exclusive and corresponds to finalized Base block
`49,477,581` at the time the run was pinned.

## Pinned artifacts and fixtures

Both deployed IPFS artifact sets remain in `artifacts/deployment/`, with CID,
SHA-256, and byte-length locks. Verify them and the canonical Base receipt
fixtures with:

```bash
./scripts/verify-pinned-artifacts.sh
./scripts/verify-base-fixtures.mjs
```

Refresh event fixtures only when intentionally re-verifying canonical Base
blocks:

```bash
./scripts/capture-event-fixtures.mjs
```

Additional extraction evidence:

- [`docs/deployed-artifacts.md`](docs/deployed-artifacts.md)
- [`docs/event-decoder-validation.md`](docs/event-decoder-validation.md)
- [`fixtures/base-ranges.json`](fixtures/base-ranges.json)

For Graph Node-native generation, parity evidence, the 100,000-block benchmark,
and production backfill procedures, use
[`pinax-network/substreams-graph-node-backfill`](https://github.com/pinax-network/substreams-graph-node-backfill).
