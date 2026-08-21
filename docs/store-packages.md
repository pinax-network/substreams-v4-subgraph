# Store package ownership and release contract

This repository is the canonical source and release location for every
Substreams module used by the Uniswap v4 Base Graph Node backfill. The native
Graph Node reducer and PostgreSQL loader remain separate consumers in
[`pinax-network/substreams-graph-node-backfill`](https://github.com/pinax-network/substreams-graph-node-backfill).

## Package graph

```text
uniswap-v4-base-backfill-v0.1.0.spkg
  map_events
    |
    v
uniswap-v4-base-state-stores-v0.1.0.spkg
  store_pool_tick
  store_pool_transaction_count
  store_pool_liquidity
  store_tick_liquidity (legacy bounded-range package)
    |
    +--> uniswap-v4-base-store-fed-reducer-v0.1.0.spkg
    |      map_reducer_inputs
    |
    +--> uniswap-v4-base-store-state-reducer-v0.4.0.spkg
           16 store_tick_liquidity shards
           pool sqrt-price and token-decimal Stores
           map_store_state_inputs
             |
             v
           uniswap-v4-base-nul-metadata-audit-v0.1.0.spkg
             map_nul_metadata_audit (evidence-only)
```

The production default is `map_store_state_inputs`. It emits the compact
protobuf input consumed by the native reducer. The Substreams layer owns
cacheable primitive and derived state; the native reducer still owns Graph
Node entity save order, temporal versions, POI, VIDs, clamps, checkpoints, and
PostgreSQL transactions.

## Why the v0.4.0 release is byte-identical

The Store packages were initially built and certified while their source lived
in the internal Graph Node backfill repository. Release `v0.4.0` republishes
those exact bytes from this repository. This preserves:

- every SPKG SHA-256 value;
- the event and Store module hashes;
- the existing production Substreams cache;
- the package consumed by the completed 100,000-block and full-clone parity
  certificates;
- deterministic restart and continuation across the repository migration.

The historical repository URL embedded in those immutable package bytes is
informational. Rewriting it would change the SPKG digest. New package versions
must use `https://github.com/pinax-network/substreams-v4-subgraph` and import or
`use` immutable earlier modules when their cache identity must be retained.

The authoritative digests and module hashes are recorded in
`packages/base-uniswap-v4-v0.4.0.json`.

Historical source required to explain the earlier packages is retained under
`legacy/`: the v0.2.0 probe used by the first State Stores package and the
v0.3.0 probe used by the Store-fed package. The current top-level crates are
the production Store-state implementation.

## Build and validate

Run the complete source validation:

```bash
make validate
```

Host builds prove Rust behavior, formatting, manifest validity, and cache
boundaries. They are not production release artifacts because Rust WASM may
contain build paths.

Stage and verify the immutable certified packages:

```bash
make release-packages
./scripts/verify-release-packages.sh dist
```

The staged assets must match all three locked SHA-256 values before a tag is
published. Their exact bytes are retained under `release/v0.4.0/` because Rust
WASM can encode its historical build path; a later source rebuild is behavioral
evidence, not a replacement for a certified artifact.

## Release and consumer rules

- GitHub release `v0.1.0` remains the canonical event package release.
- GitHub release `v0.4.0` is the canonical Store package bundle.
- Never delete or replace a released asset.
- Never use `--clobber` for immutable SPKG release assets.
- Consumers pin repository, release, filename, SHA-256, package version,
  module name, module hash, and output protobuf.
- Runtime images may vendor a verified SPKG for hermetic execution, but they
  must not become its canonical release location.

## Tick Store sharding

The legacy single `store_tick_liquidity` module is retained only for the
earlier Store-fed package. It crosses the Substreams server's 2 GiB per-Store
limit over the full Base history. The production Store-state package replaces
it with 16 deterministic shards selected by the first Pool-ID byte modulo 16.
Each shard preserves the legacy key and delta encoding while bounding storage.

Changing the assembler binary must leave every Store hash unchanged. Changing
the Tick-shard binary may change only the 16 Tick-shard hashes and downstream
maps. The cache-boundary tests enforce both invariants.

The NUL metadata audit is a downstream-only certification package. It imports
the complete immutable v0.4.0 graph and adds one map, so changing its WASM must
not change any imported module hash. It is never a production reducer-input
replacement.
