# Graph Node v0.44 reference oracle

This environment runs the exact Graph Node release used by Pinax with Postgres
16, local IPFS, the production provider class (filtered Base Firehose plus
archive/traces RPC), and only loopback host ports. It is not connected to the
live deployment database.

The default GraphQL/status ports are `18100`/`18130`; set
`ORACLE_GRAPHQL_PORT` and `ORACLE_STATUS_PORT` before running the scripts if
either conflicts locally. Admin and metrics ports can likewise be changed with
`ORACLE_ADMIN_PORT` and `ORACLE_METRICS_PORT`.

## Root mode

Root mode deploys the pinned graft parent from its real start block and pauses it
at a small, hash-pinned checkpoint:

```bash
export PINAX_API_KEY=...
./oracle/up.sh
./oracle/deploy-root.sh
./oracle/export-fixture.sh root /tmp/v4-root-fixture
./oracle/verify-determinism.sh root
```

The root fixture covers Base blocks 25,350,988 through 25,352,561 and includes
the first PoolManager Initialize event. The export contains Graph Node's native
Parquet dump, canonical temporal version rows, per-block creates/updates/deletes
that can be inferred from those rows, a full checkpoint snapshot for all 17
root entity types, representative GraphQL results, and a diagnostic POI query.
Repeated canonical exports must be byte-identical. A delete followed by a
recreate in one block is indistinguishable from an update in Graph Node's
temporal rows and is intentionally reported as such.

GraphQL state is selected by the pinned checkpoint number because a forced
rewind to an old block does not repopulate Graph Node's shallow chain-cache
lookup by hash. The export separately asserts that `_meta.block.hash` is the
pinned canonical hash before dumping or querying.

## Child/post-graft mode

The graft copy is inclusive: parent/child state through block 26,990,278 is the
seed, the child head is set to that block, and new child handlers begin at block
26,990,279.

An approved dump of the existing child seed avoids replaying the dense graft
parent. Do not build that seed against the live writer:

Do not build the seed by starting the child from empty state. Produce it by:

1. taking a database-level copy of the existing child deployment into a
   disposable Postgres instance;
2. stopping its Graph Node writer;
3. rewinding only that disposable copy to block 26,990,278 with hash
   `0x3365…8948`;
4. running Graph Node v0.44 `graphman dump` there;
5. confirming `metadata.json` contains the original child deployment, graft
   metadata, and the exact seed head.

This is intentionally not automated against production: `graphman dump` uses a
single transaction and can load a large live database. No live `sgd1246` read or
write is part of this repository's normal commands.

Restore the approved seed into this disposable local oracle, advance to the
first-child fixture checkpoint, and export it:

```bash
./oracle/restore-child-seed.sh /path/to/approved-child-seed-dump
./oracle/export-fixture.sh child /tmp/v4-child-fixture
./oracle/verify-determinism.sh child
```

Replaying the parent locally is intentionally not offered as the normal path:
the deployed v4 mapping has trigger-bearing activity in nearly every Base block,
which reproduces the same single-thread WASM bottleneck this prototype is meant
to bypass.

## Shutdown

```bash
./oracle/down.sh
```

`docker compose down -v` is deliberately not wrapped because it deletes the
local oracle database and IPFS volumes. Use it manually only when that data is
no longer needed.

The compose file pins Graph Node v0.44.0, Postgres 16.11, and Kubo v0.34.1 by
repository digest. The Graph Node tag resolves to source commit
`77823b5a7adc4eac4abf03202c8dd7cb00dea31d`.
