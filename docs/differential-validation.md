# Differential validation

The parity harness has two independent gates. Gate A compares Rust replay with
the paused Graph Node mapping result. Gate B compares Graph Node's restored
PostgreSQL representation with the same paused source deployment. Both commands
verify the exact deployment ID and refuse to read from an active oracle writer.

## Gate A: mapping state, order, and POI

Run every range pinned in `fixtures/base-ranges.json`:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export SUBSTREAMS_API_TOKEN=...
make verify-all-state-ranges
```

Each range exports only the state active immediately before its first block,
then replays Substreams events through its inclusive end. The comparator fails
on a field difference, missing or unexpected write, write-order difference, or
final POI digest difference. Protobuf JSON's omitted zero-valued indices and
ordinals are reconstructed as zero before replay.

The PostgreSQL export ends with a manifest containing exact counts for table
metadata, POI seed/checkpoint rows, seed versions, and expected rows. A partial
`kubectl exec` stream is rejected and retried up to three times before any
Substreams replay; transport truncation can therefore never appear as a parity
result.

The certified 2026-08-02 run is:

| Range | Blocks | Expected/changed entity writes | Differences |
| --- | ---: | ---: | ---: |
| first-child-core-events | 242 | 798 / 798 | 0 |
| arrakis-hook | 1 | 18 / 18 | 0 |
| position-unsubscriptions | 26 | 686 / 686 | 0 |
| position-subscription | 1 | 59 / 59 | 0 |
| **Total** | **270** | **1,561 / 1,561** | **0** |

This covers all seven handled event kinds, the inclusive graft boundary,
multiple events per transaction and block, immutable creations, mutable
rewrites, zero-valued indices, and every one of the 18 schema entities.

Set `PARITY_REPORT=/absolute/path/report.json` to retain the deterministic
machine-readable report. CI runs the network-independent canonical receipt and
reducer fixture suite and uploads both JSON and readable test output as the
`offline-parity-report` artifact.

## Gate B: restored Graph Node database

First restore the bounded native Parquet fixture into the disposable Graph Node
v0.44 oracle. Then run:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
export PINAX_API_KEY=...
make verify-restored-parity LOCAL_SCHEMA=sgd3
```

The certified bounded restore compared 3,073 rows in all 20 physical tables:
13 mutable tables including `poi2$`, six immutable tables, and
`data_sources$`. Every canonical row matched. The comparison uses PostgreSQL's
own `jsonb` rendering, so bytea, numeric, arrays, nulls, columns, temporal
ranges, and immutable block values are checked in their actual storage types.

### Explicit VID invariant

The sole excluded column is `vid`. Graph Node v0.44's own `graphman restore`
path does not insert dump VIDs for this deployment's legacy
`specVersion: 0.0.4` schema; it allocates each table's database sequence during
restore. Therefore even a Graph Node-produced dump is renumbered when restored
into a fresh schema. VIDs are internal surrogate keys and are not query-visible.
The Parquet writer nevertheless assigns stable table-local VIDs because Graph
Node requires them for chunk order and they provide deterministic resume
boundaries. No entity field, version order, range, POI value, or GraphQL result
is excluded.

### Checkpoint range normalization

The read-only source clone was two blocks ahead of the bounded fixture. If a
source mutable version closes after the certified checkpoint, its upper bound
is normalized to open for this comparison. That is the state the source had at
the checkpoint and exactly what the restored, checkpoint-paused schema stores.

Set `PARITY_REPORT=/absolute/path/report.json` to retain the physical comparison
report. A mismatch report includes per-table counts and the first canonical row
diff.
