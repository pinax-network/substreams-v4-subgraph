# Uniswap v4 Base parity contract

This project targets one immutable deployment artifact:
`Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB`. Current upstream source is
not an authority. The checked-in manifest, schema, ABIs, and mapping WASMs are.

“1:1” has two separately tested levels. Logical mapping parity is necessary but
does not prove that Graph Node can serve or continue the restored deployment.
Physical Graph Node compatibility is the required prototype outcome.

## Gate A: logical mapping parity

For every handled log, the Rust implementation must produce the same ordered
store operations as Graph Node executing the pinned WASM. Compare:

- block number, block hash, timestamp, transaction hash/index/from, log index,
  data-source address, handler, and per-handler operation ordinal;
- operation (`set` or `remove`), exact entity type, and exact case-sensitive ID;
- the presence and value of every field, including relation IDs;
- immutable entity creation, mutable entity rewrites, and any explicit removal;
- handler errors and the resulting committed or discarded operations under the
  manifest's `nonFatalErrors` feature.

The canonical stream is newline-delimited JSON. Each record has this shape:

```json
{
  "block": { "number": 26990280, "hash": "0x...", "timestamp": 0 },
  "transaction": { "hash": "0x...", "index": 109, "from": "0x..." },
  "log": { "index": 177, "address": "0x..." },
  "ordinal": 0,
  "handler": "handleModifyLiquidity",
  "operation": "set",
  "entity": { "type": "Pool", "id": "0x..." },
  "fields": {}
}
```

Records sort by block number, transaction index, log index, then operation
ordinal. The ordinal is the mapping's store-call order within one handler. No
comparator may sort operations by entity type or ID, because doing so would hide
ordering differences that affect POI and same-block state reads.

### Canonical values

- `ID` and `String`: exact UTF-8 strings; no case folding or whitespace changes.
- `Bytes`: lowercase `0x`-prefixed even-length hexadecimal. Empty bytes are
  `0x`. Address-valued bytes retain all 20 bytes.
- `BigInt`: base-10 string, `0` for zero, optional leading `-`, no `+` or leading
  zeroes.
- `BigDecimal`: the exact Graph value after Graph's decimal128 arithmetic. The
  comparator normalizes notation to a sign, integer coefficient, and exponent,
  but applies no tolerance and performs no new rounding.
- `Int`: signed JSON integer in Graph's 32-bit domain.
- `Boolean`: JSON boolean.
- enums: exact schema spelling as a string.
- lists: ordered arrays whose elements follow the scalar rule; list order is
  significant.
- relations: the referenced entity's exact ID string.
- null and unset are distinct. A present field with JSON `null` cannot compare
  equal to an absent field.

An immutable entity may be created once and may not be updated or removed. A
mutable `set` is a complete entity image, matching `store.set`; it is not a
partial SQL patch. Although the deployed source has no active `store.remove`,
the format reserves `remove` and the comparator must not silently ignore it.

Gate A also compares full snapshots of all 18 schema entities at each checkpoint.
An entity type with no rows must be represented as an empty set, not omitted.

## Host calls and deterministic inputs

The deployed PoolManager WASM imports `ethereum.call` for ERC-20 metadata only:

- `symbol() -> string`, then `symbol() -> bytes32` on revert;
- `name() -> string`, then `name() -> bytes32` on revert;
- `totalSupply() -> uint256`;
- `decimals() -> uint8`, accepted only when less than 255.

Native currency and statically configured tokens bypass some calls. Failed
symbol/name calls fall back to `unknown`; failed total supply returns zero;
failed or invalid decimals return null and can cause the initialize handler to
return without creating the token/pool. Calls must execute against the exact
historical block state used by Graph Node. Cached lookup values therefore need
the block hash and call tuple in their key. The deployed WASMs do not call an
Arrakis TVL method.

Other host imports are deterministic numeric/conversion helpers, `store.get`,
`store.set`, logging, and `dataSource.network`. The PositionManager and Arrakis
WASMs do not import `ethereum.call`.

## Gate B: Graph Node v0.44 physical compatibility

Logical snapshots do not prove this gate. The Parquet/restore path must also
match or correctly reconstruct:

- every mutable entity version and half-open block range;
- immutable `block$` values, causality region where applicable, deterministic
  `vid` ordering, and table sequences;
- ordered `Poi$` events and sampled/certified POI digests;
- dynamic data sources if any, graft base and block, manifest/schema identity,
  deployment head, cursor, health, errors, entity count, and history settings;
- GraphQL responses through Graph Node v0.44 for the original deployment ID;
- next-block processing, fork revert, restart/resume, and exclusive writer
  ownership.

Gate B is zero-difference unless a Graph Node invariant is explicitly proven to
tolerate a physical difference. Such an exception must be documented before it
is accepted; snapshot equality alone is never sufficient.

## Pinned fixtures

[`fixtures/base-ranges.json`](../fixtures/base-ranges.json) pins four ranges and
their boundary hashes. Together they cover the graft boundary, Initialize,
ModifyLiquidity, Swap, Transfer, Subscription, Unsubscription, Arrakis hook
creation, multiple events in a block/transaction, and high-density blocks.

Post-graft fixtures require state at the block immediately preceding the range.
The oracle must export that state from a disposable copy of the grafted
deployment; starting the child from an empty store is invalid.
