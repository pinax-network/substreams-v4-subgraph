# Exact entity reducer

The deployed mapping semantics are implemented as a deterministic Rust reducer
after `map_events`. This is an intentional Substreams boundary, not a generic
SQL sink transformation.

Substreams modules form a directed acyclic graph and a store cannot depend on
itself. The Uniswap mapping requires arbitrary read-after-write entity state:
derived prices traverse token and pool entities, multiple handlers update the
same records, and every BigDecimal operation must use Graph Node v0.44's pinned
rounding. Native additive/set store policies cannot express that reducer
exactly.

The supported pipeline is therefore:

1. Base blocks are decoded and historical ERC-20 calls are executed in the
   parallel, cacheable `map_events` module.
2. The ordered protobuf stream is replayed by `EntityState`, one handler at a
   time, using an imported graft checkpoint as its initial state.
3. Every `store.set` image is retained in mapping order for POI generation, and
   the current entity maps provide the same same-block reads as Graph Node's
   entity cache.
4. A later writer coalesces final per-block modifications and creates native
   Graph Node Parquet, temporal versions, metadata, and POI tables.

This keeps the expensive block scanning, ABI decoding, and historical RPC work
parallel and cacheable. Only the state-dependent reducer is sequential, as it
is in Graph Node, but it runs as native Rust without AssemblyScript WASM or SQL
writes in the hot path.

## Source authority

The implementation follows deployed artifacts first and the reconstructed
Uniswap source at commit `773849724baa7afdc50848978bd8d7087ccef000`
second. It implements:

- PoolManager, Bundle, Token, Pool, Tick, and all six interval entity types;
- immutable Transaction, Swap, ModifyLiquidity, Subscribe, Unsubscribe, and
  Transfer entities from the child schema;
- Position and ArrakisHook;
- Base-specific stable pool, whitelist, native currency, and Zora hook logic;
- exact tick, sqrt-price, liquidity delta, fee, TVL, volume, and derived-native
  arithmetic;
- exact handler return behavior when token decimals are unavailable;
- exact Graph Node v0.44 BigDecimal normalization through the same
  `bigdecimal` 0.1.2 arithmetic and 34-digit precision.

`EntityState::insert_seed` loads canonical state through graft block 26,990,278
without emitting child mapping writes. Child events begin at 26,990,279.

## Validation

Offline tests cover decimal intermediate rounding, Uniswap tick boundaries,
all 18 entity types, deterministic replay, save ordering, and the decimals-null
early-return path.

The live metadata path was exercised at Base block 25,352,561. Substreams
returned the exact first root Initialize trigger and historical metadata for
ETH/flETH. Shared scalar and arithmetic fields were checked against the Graph
Node v0.44 native Parquet oracle paused at block hash
`0x309c5dd0325515927305c93df7bfa9e3cef7adb15cf1ae3f45f227db15d830f8`.
That graft parent used different Base whitelist configuration, so it is not a
complete entity oracle for the target child deployment.

Post-graft differential validation uses read-only, targeted rows from the
paused `sgd1246` copy as seed and expected data. The workflow never writes to
that database; restoration is reserved for a disposable child clone. Run it
with `make verify-state-parity`. The verifier also requires the corresponding
Graph Node workload to remain at zero replicas and confirms the deployment ID
before it reads any entity rows.

The canonical first-child run replays blocks 26,990,279 through 26,990,520
(242 blocks) and compares every key touched by a handler, including saves that
do not create a new temporal SQL version because their value is unchanged. The
verified result is 798 expected entities, 798 changed entities, and zero value
or entity-set mismatches.
