# Event decoder validation

Validation date: 2026-08-03

This milestone validates raw trigger extraction for the exact deployed Base
subgraph. It does not claim entity-state, POI, or native-Parquet parity; those
gates are owned by
[`pinax-network/substreams-graph-node-backfill`](https://github.com/pinax-network/substreams-graph-node-backfill) and
remain separate gates.

## Reproducible build

- Rust: 1.90.0 (`rust-toolchain.toml`)
- Substreams Rust SDK: 0.7.6
- Substreams Ethereum: 0.11.1
- certified v0.1.0 release SPKG SHA-256:
  `75b810d18ec1dc78ca5535b2cc56828334c873b93d39500f93ca036499048453`
- certified v0.1.0 `map_events` module hash:
  `8fbf7c14cefe3d5b0c249a8cf4566bcaa8d2ed26`

`make validate` rebuilds the WASM and package from a clean checkout. On the
handoff branch the SPKG is written to
`spkg/uniswap-v4-base-backfill-v0.1.1.spkg` and is intentionally not committed.
The backfill runtime continues to pin the immutable v0.1.0 release above.

## Offline tests

The canonical receipt fixture at `fixtures/event-logs.json` was captured from
the hashes pinned in `fixtures/base-ranges.json`. Unit tests cover:

- Initialize, ModifyLiquidity, Swap, Subscription, Unsubscription, Transfer,
  and LogCreatePrivateHook;
- multiple relevant events in one transaction;
- malformed event data and a correct signature emitted by the wrong address;
- minimum/maximum int24 and int128 values; and
- explicit sorting by Graph Node v0.44's block-global log index.

The tests need no network after the fixture is committed.

## Bounded live Base run

`make verify-live` built the release WASM and ran all pinned ranges against
`base-substreams-tier1-prod.kan-sst2.pinax.io:443`. It compared decoded counts,
boundary block hashes, anchor transactions/logs, deployed source addresses, and
strict per-block trigger order.

| Fixture | Inclusive blocks | Decoded events | Expected breakdown |
| --- | ---: | ---: | --- |
| first-child-core-events | 26990279–26990520 | 257 | Initialize 1, ModifyLiquidity 14, Swap 238, Transfer 4 |
| arrakis-hook | 28626556 | 3 | ModifyLiquidity 1, Swap 1, LogCreatePrivateHook 1 |
| position-unsubscriptions | 31211085–31211110 | 245 | ModifyLiquidity 85, Swap 158, Unsubscription 2 |
| position-subscription | 31259107 | 8 | Swap 7, Subscription 1 |

All four runs passed. The first range starts at the first child-executed block;
the graft seed at block 26990278 remains inclusive base state and is not
re-executed as a child handler block.

## Runtime handoff differential

Moving the native Graph Node runtime out of this crate changes the compiled
WASM bytes and therefore the v0.1.1 module hash. To exclude a semantic change,
the released v0.1.0 package and the slimmed v0.1.1 package were run against the
same endpoint for all four ranges above. Their emitted `map_events` JSONL
records were compared byte-for-byte after excluding CLI progress lines.

| Fixture | Emitted blocks compared | Result |
| --- | ---: | --- |
| first-child-core-events | 134 | identical |
| arrakis-hook | 1 | identical |
| position-unsubscriptions | 26 | identical |
| position-subscription | 1 | identical |

The comparison covered 162 emitted blocks and every supported event type, with
no output differences.
