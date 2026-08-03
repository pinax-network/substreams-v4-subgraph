# Graph Node-native Parquet

The native sink writes the directory format consumed by Graph Node v0.44's
`graphman restore`. It is not generic analytics Parquet: column names, Arrow
types, temporal version ranges, immutable `block$` columns, table-local `vid`
sequences, clamp files, Zstandard compression, metadata, graft pointers, and
`Poi$` all follow Graph Node's dump contract.

The target deployment uses Graph Node's legacy proof-of-indexing stream. The
writer detects that format from the graft seed, uses Graph Node's pinned
`stable-hash` revision, and retains fast-hash support for other v0.44 snapshots.

## Bounded proof fixture

The first-child fixture is deliberately small enough for repeated differential
tests. It reads a selective graft checkpoint from the paused `sgd1246` clone,
replays blocks 26,990,279 through 26,990,520 with Substreams, verifies all 798
handler-touched final entities, and materializes their full temporal history.

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export SUBSTREAMS_API_TOKEN=...
./scripts/build-parquet-fixture.sh /tmp/uniswap-v4-native-dump
```

The command refuses an existing output path and refuses an active Graph Node
oracle workload. It never writes to the source database.

Restore into the disposable Graph Node v0.44 oracle:

```bash
export PINAX_API_KEY=...
./oracle/restore-native-fixture.sh \
  /tmp/uniswap-v4-native-dump \
  oracle/native-parquet-fixture-1
```

The restore uses the original deployment ID, pauses it before the oracle starts,
and asserts the exact head block, head hash, deployment, and indexing-error
state through GraphQL.

Compare the restored PostgreSQL rows with the paused source copy:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export PINAX_API_KEY=...
make verify-restored-parity LOCAL_SCHEMA=sgd3
```

The comparison covers all 19 entity/POI tables plus `data_sources$`. It removes
only `vid`, because Graph Node v0.44 itself ignores supplied dump VIDs and
allocates new per-table VIDs when restoring this deployment's legacy
`specVersion: 0.0.4` schema. This is a proven Graph Node restore invariant, not
a comparator tolerance: VIDs are internal, are not GraphQL-visible, and the
writer still uses deterministic VIDs for chunk ordering and resumable output.

When the paused source has indexed beyond the fixture checkpoint, a source
version whose upper range is after the checkpoint is normalized to an open
range. The restored database is paused exactly at the checkpoint, so its same
version is correctly open there.

## Scope boundary

This fixture proves native dump compatibility and exact results for the bounded
range. Its graft seed contains only entities needed by that range, so it must
not be used as a production replacement dump. A production backfill must seed
the complete active graft state (and required retained history), process the
entire child range, pass the full differential harness, and be restored first
into a disposable clone of the existing deployment. Production `sgd1246` must
not be replaced without a separately approved cutover.
