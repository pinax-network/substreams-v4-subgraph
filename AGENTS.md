# Agent guide

This repository owns the complete Substreams half of the Uniswap v4 Base
Graph Node backfill. It publishes the immutable event, Store, and Store-fed map
packages consumed by `pinax-network/substreams-graph-node-backfill`.

## Ownership boundary

- Keep EVM extraction, cacheable Store writers, downstream Substreams maps,
  protobuf contracts, manifests, and SPKG release automation here.
- Keep Graph Node entity reduction, POI, PostgreSQL COPY/Parquet output,
  lifecycle controls, and physical parity certification in
  `pinax-network/substreams-graph-node-backfill`.
- Keep Kubernetes workflow and image pins in `pinax-network/k8s-subgraphs`.

Do not add PostgreSQL credentials, Graph Node database writes, restore logic,
or cluster mutation to this repository.

## Immutable production packages

`packages/base-uniswap-v4-v0.4.0.json` is the machine-readable release lock.
The three Store package files in release `v0.4.0` must remain byte-identical to
the packages first certified by the Graph Node backfill runtime:

- `uniswap-v4-base-state-stores-v0.1.0.spkg`;
- `uniswap-v4-base-store-fed-reducer-v0.1.0.spkg`;
- `uniswap-v4-base-store-state-reducer-v0.4.0.spkg`.

Their embedded package URL points at the historical build repository. Do not
edit the frozen manifests merely to rewrite that metadata: doing so changes the
SPKG byte stream. New package versions must use this repository URL while
importing or using the released modules needed to preserve cache hashes.

## Cache contract

- Never change a released Store writer in place.
- Keep each Store writer binary isolated from downstream map binaries.
- Preserve all 16 Tick-liquidity shards. The legacy single Store exceeds the
  server's 2 GiB Store limit on the full Base history.
- A downstream map change must not alter unrelated Store hashes.
- Treat module hashes, SPKG SHA-256 values, package versions, module names,
  initial blocks, update policies, value types, and protobuf output types as a
  compatibility contract.

## Workflow

1. Start from clean `main` on a feature branch.
2. Make the smallest compatible change.
3. Run `make validate`.
4. For release changes, run `make release-packages` and
   `./scripts/verify-release-packages.sh dist`.
5. Inspect `git diff`, `git diff --check`, and the staged file list.
6. Open a substantive PR and do not merge with failing required checks.
7. Verify the release asset digests after publishing.

Never call a host-built SPKG production-identical. Rust WASM can embed build
paths. The immutable certified package bytes are retained under
`release/v0.4.0/`; source builds validate behavior and cache boundaries but do
not replace those release artifacts.
