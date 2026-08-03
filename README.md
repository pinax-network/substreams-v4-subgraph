# Uniswap v4 Base Substreams

This repository builds the checksum-pinned Substreams event extractor for the
Base Uniswap v4 deployment
`Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB`.

Its responsibility ends at deterministic `map_events` output. The Graph Node
state reducer, proof-of-indexing implementation, native Parquet writer,
segmented backfill, restore wrappers, and certification harness now live in the
internal
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

The output is an event contract, not Graph Node entity changes. Consumers that
need Graph Node-compatible PostgreSQL state must use the matching backfill
runtime release.

## Build and validate

The repository pins Rust 1.90.0 and all direct dependencies:

```bash
make validate
```

This checks generated protobuf bindings, runs extraction tests, applies
formatting and strict Clippy, builds the WASM, and writes:

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
