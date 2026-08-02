# Event decoder validation

Validation date: 2026-08-02

This milestone validates raw trigger extraction for the exact deployed Base
subgraph. It does not claim entity-state, POI, or native-Parquet parity; those
remain separate gates.

## Reproducible build

- Rust: 1.90.0 (`rust-toolchain.toml`)
- Substreams Rust SDK: 0.7.6
- Substreams Ethereum: 0.11.1
- WASM SHA-256:
  `a0f325dc5384f4a48e4d3f7822a7384011adc3bf1b86c716a6c7df58fb5ac7a4`
- SPKG SHA-256:
  `c604051c6b7416e800181b3ee1afcd6a68f8bd13081be592ef9877a87b16cc0f`
- SPKG module hash:
  `c26aa7a6eed746b310561adfdf3d0c06a6572f28`

`make validate` rebuilds the WASM and package from a clean checkout. The SPKG
is written to `spkg/uniswap-v4-base-backfill-v0.1.0.spkg` and is intentionally
not committed.

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
