# Pinned deployed artifacts

The deployment CID is the source of truth. Files under
`artifacts/deployment/Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB/`
are the exact bytes returned by The Graph's IPFS gateway on 2026-08-02.
`artifacts.lock.json` records every CID, SHA-256 digest, and byte length.

Run the offline verifier after installing `jq` and Kubo/IPFS:

```bash
./scripts/verify-pinned-artifacts.sh
```

The verifier recomputes each SHA-256 and CIDv0 locally, checks the manifest's
complete reference set, confirms all 18 entity declarations, and validates the
three WASM headers. It does not trust the gateway while verifying.

Verify that the pinned Base blocks are still canonical and contain the exact
expected event counts:

```bash
./scripts/verify-base-fixtures.mjs
```

Set `BASE_RPC_URL` to use a different Base archive RPC. The default is the public
Base mainnet endpoint.

To re-download one object independently, use its CID from the lock file:

```bash
curl -fsSL "https://ipfs.network.thegraph.com/ipfs/$CID" > /tmp/artifact
ipfs add --only-hash --cid-version=0 -Q /tmp/artifact
```

## Deployment configuration

| Data source | Address | Start block |
| --- | --- | ---: |
| PoolManager | `0x498581fF718922c3f8e6A244956aF099B2652b2b` | 25,350,988 |
| PositionManager | `0x7C5f5A4bBd8fD63184577525326123B519429bDc` | 25,350,993 |
| ArrakisHookFactory | `0xeF129a430032C8183abA158C1a70799e3b840dF9` | 28,450,225 |

The child grafts from
`QmS1ehFzXTD9eA1f1EgjZvdyAj2EHtVNMrEN91H3pLuHMy` at Base block
26,990,278. It enables `nonFatalErrors` and `grafting`.

## Upstream source relationship

The best matching source is Uniswap commit
`773849724baa7afdc50848978bd8d7087ccef000` (2025-08-14). The relationship is
classified **reconstructed**, not byte-verified:

- `schema.graphql` is byte-identical;
- all six ABI JSON documents are semantically identical after canonical JSON
  serialization;
- the generated manifest has the same network, addresses, start blocks, graft,
  handlers, entities, and mapping entrypoints;
- all rebuilt WASM sections, imports, function counts, data sizes, control flow,
  and non-diagnostic operands match; compiler-generated source line operands
  passed to `abort` are one line lower in the local rebuild;
- because the rebuilt WASM hashes are not byte-identical, the checked-in deployed
  WASMs—not the source commit—remain the executable oracle.

Reconstruction used the commit's frozen Yarn lock, Yarn 1.22.22, and its
documented `yarn build` command. This finding is sufficient to port readable
logic while preserving a hard differential test against the deployed binaries.

## Version pins

- Graph Node: `v0.44.0`, commit
  `77823b5a7adc4eac4abf03202c8dd7cb00dea31d`.
- Optional SQL Sink components for a future live tail only: `v4.13.1`, commit
  `c05b15e5c82dadb6c74efe12087a5cc1bc215c9a`.

Historical backfill uses Graph Node v0.44 dump-compatible Parquet. CSV, generic
analytics Parquet, a separate query database, and SQL Sink as the historical
loader are outside the accepted architecture.

The reducer, native writer, restore environment, and certification workflow are
maintained in
[`pinax-network/substreams-graph-node-backfill`](https://github.com/pinax-network/substreams-graph-node-backfill).
