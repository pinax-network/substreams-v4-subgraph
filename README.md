# Substreams Uniswap v4 Base backfill

This repository is a proof of concept for replacing the historical Graph Node
WASM backfill of Uniswap v4 on Base with parallel Substreams computation, then
restoring Graph Node v0.44-compatible Parquet into the existing deployment.

The target is the original deployment
`Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB`. A separate database,
alternate deployment ID, or alternate query service is not considered success.

The first completed building block pins the deployed artifacts and defines what
“1:1” means:

- [deployed artifact inventory](docs/deployed-artifacts.md)
- [logical and physical parity contract](docs/parity-contract.md)
- [pinned Base fixture ranges](fixtures/base-ranges.json)

Verify the artifact inventory locally:

```bash
./scripts/verify-pinned-artifacts.sh
./scripts/verify-base-fixtures.mjs
```

Implementation will proceed from the deployed binaries and reconstructed source
commit, through a Graph Node reference oracle, Rust event/state modules,
Graph-compatible Parquet, differential tests, and disposable-clone restore.
