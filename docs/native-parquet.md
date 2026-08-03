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

To rehearse replacement of an already registered copy of that same deployment,
use Graph Node's replacement path rather than creating another logical target:

```bash
RESTORE_MODE=replace ./oracle/restore-native-fixture.sh \
  /tmp/uniswap-v4-native-dump \
  oracle/native-parquet-fixture-1
```

This invokes Graph Node v0.44's native `graphman restore --replace`; the
deployment hash and GraphQL serving name remain unchanged while Graph Node
recreates its physical deployment schema. The wrapper then reconciles every
per-table `vid` sequence to the restored maximum before Graph Node starts.
This is required because `restore --replace` can retain a sequence position
from the replaced schema even though the restored table rows were renumbered.

Compare the restored PostgreSQL rows with the paused source copy:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export PINAX_API_KEY=...
make verify-restored-parity LOCAL_SCHEMA=sgd3
```

The comparison covers all 19 entity/POI tables plus `data_sources$`. It removes
only `vid`, because Graph Node v0.44 itself renumbers dump rows when restoring
this deployment's legacy `specVersion: 0.0.4` schema. This is a proven Graph
Node restore invariant, not a comparator tolerance: VIDs are internal, are not
GraphQL-visible, and the writer still uses deterministic VIDs for chunk
ordering and resumable output. The restore wrapper separately verifies that
each resulting sequence is positioned at its table's maximum, which protects
the first normal Graph Node write after replacement.

When the paused source has indexed beyond the fixture checkpoint, a source
version whose upper range is after the checkpoint is normalized to an open
range. The restored database is paused exactly at the checkpoint, so its same
version is correctly open there.

## Resumable segmented generation

The append path starts from a valid Graph Node-native dump and preserves its
existing chunks. Each contiguous Substreams segment produces:

- new entity and `Poi$` chunks;
- clamps for active mutable versions from the prior segment;
- a compact resume checkpoint containing only active mutable state, generated
  table-local VIDs, and the latest POI digest;
- an atomic `.substreams-append.json` journal while an append is incomplete.

Completed files are never rewritten. The journal records their row counts and
SHA-256 digests. `metadata.json` remains at the previous certified head until
all 19 entity/POI tables complete, then it is replaced atomically and the
journal is removed. Repeating a completed append is idempotent.

Run the two-segment bounded proof:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export SUBSTREAMS_API_TOKEN=...
STOP_AFTER_TABLES=7 ./scripts/build-resumable-parquet-fixture.sh \
  /tmp/uniswap-v4-resumable-dump
```

The optional stop leaves seven completed tables on disk, then the same command
continues from the journal. The certified fixture splits at block 26,990,350,
resumes through 26,990,520 from the generated checkpoint, restores with the
unmodified Graph Node v0.44 reader, and matches all 3,073 PostgreSQL rows in all
20 physical tables.

The next segment must always consume the previous generated checkpoint. A
fresh SQL snapshot has Graph Node's source VIDs, which are not the generated
dump's VIDs and would clamp the wrong rows after a segment boundary. The
physical differential explicitly caught and rejects that invalid construction.

After restoring the interrupted/resumed artifact, exercise the normal Graph
Node writer and revert path:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export PINAX_API_KEY=...
LIFECYCLE_REPORT=/tmp/v4-lifecycle.json \
  ./oracle/verify-restored-lifecycle.sh \
    sgd7 oracle/native-parquet-resume-script-1
```

The certifier processes the pinned next ten Base blocks with Graph Node v0.44,
compares all 20 physical tables with the paused source, restarts Graph Node,
rewinds every subsequent entity/POI row, and replays the range. Canonical
GraphQL responses must remain byte-identical across restart, rewind, and replay.

## Complete-history production shape

A production artifact starts with a native `graphman dump` of the disposable
child clone rewound to the inclusive graft block. That dump retains the large
pre-graft mutable history and immutable rows without converting roughly 250 GB
of PostgreSQL history through JSON. The reducer separately loads all active
mutable rows at the graft checkpoint, then segmented Substreams append owns the
entire child range. This combines native historical preservation with bounded
resume checkpoints; it does not read or rewrite the live production schema.

## Scope boundary

This fixture proves native dump compatibility and exact results for the bounded
range. Its graft seed contains only entities needed by that range, so it must
not be used as a production replacement dump. A production backfill must seed
the complete active graft state (and required retained history), process the
entire child range, pass the full differential harness, and be restored first
into a disposable clone of the existing deployment. Production `sgd1246` must
not be replaced without a separately approved cutover.
