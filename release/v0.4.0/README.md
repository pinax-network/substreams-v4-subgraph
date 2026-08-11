# Immutable v0.4.0 Store package bundle

These are the exact SPKG bytes that passed the Uniswap v4 Base Store-state
Graph Node parity certification before their canonical repository ownership was
corrected.

| Asset | SHA-256 | Source provenance |
| --- | --- | --- |
| `uniswap-v4-base-state-stores-v0.1.0.spkg` | `ac3b295591e68949881509379aa99d8460134873f895187746a35ece003fa440` | Store writers plus the frozen `legacy/state-stores-v0.1.0/store-probe` source from the original v0.2.0 release |
| `uniswap-v4-base-store-fed-reducer-v0.1.0.spkg` | `7cf06bb4b73c1409c581a7f878fb832c3e15c1d82193103d8bc7609dce8b58f3` | Frozen `legacy/store-fed-v0.1.0/store-probe` source from the original v0.3.0 release |
| `uniswap-v4-base-store-state-reducer-v0.4.0.spkg` | `bc8a841dbb74f9bbc7d90826251ab766ad350170a4832fc3e9b084664d22254a` | Current Store-state, metadata, Tick-shard, and downstream map source |

Do not rebuild, replace, or clobber these files. Host and container rebuilds can
encode different source paths in Rust WASM. Use `make release-packages` to
stage and verify them against `packages/base-uniswap-v4-v0.4.0.json`.
