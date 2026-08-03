# Complete-history backfill procedure

This procedure constructs one Graph Node v0.44-native dump for the original
deployment. It never converts the retained pre-graft history to JSON and never
writes to live `sgd1246`.

## 1. Prepare the graft seed offline

On a disposable database clone of the existing deployment:

1. stop every Graph Node writer for the clone;
2. rewind the clone to block 26,990,278 with hash
   `0x3365afb2ec41d64886321de82b8fd73427fda55fcf731bf5a2888c1a2a2c8948`;
3. run Graph Node v0.44 `graphman dump` for deployment
   `Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB`;
4. verify the dump head, deployment, graft base/block, schema, manifest, all 20
   table entries, and `Poi$` metadata;
5. keep that dump immutable as the rollback/source artifact and create a
   writable reflink, filesystem snapshot, or hard-linked working tree.

The native seed retains the roughly 250 GB of temporal/immutable PostgreSQL
history in Graph Node's own format. Only active reducer state crosses JSON.

## 2. Capture active mutable state

Against the same stopped clone or the approved paused read-only source:

```bash
export KUBECONFIG=/path/to/authorized-cluster.yaml
./scripts/capture-active-seed.sh \
  /backfill/immutable-graft-seed \
  /backfill/checkpoint-26990278.json
```

The command verifies deployment identity, the stopped workload, exact pinned
schema/manifest, and the dump's graft head. A repeatable-read, read-only SQL
transaction exports every mutable entity active at the graft block plus the
active POI. The dump's own per-table max VIDs are used. An exact-count trailer
detects and retries incomplete Kubernetes transport streams.

Expected active-state scale from the approved source is about 235,000 rows,
dominated by `PoolHourData` and `TokenHourData`; immutable rows remain only in
Parquet.

## 3. Append contiguous Substreams segments

The working dump must still be at the checkpoint represented by the input
checkpoint:

```bash
export SUBSTREAMS_API_TOKEN=...
./scripts/append-parquet-segment.sh \
  /backfill/working-dump \
  /backfill/checkpoint-26990278.json \
  26990279 27090278 \
  /backfill/checkpoint-27090278.json
```

Repeat with the prior output checkpoint and the next contiguous range. The
script resolves and stores the canonical Base end hash. Substreams cache makes
reprocessing a failed segment inexpensive; the Parquet append journal prevents
completed table files from being rewritten.

The checkpoint intentionally contains only:

- active mutable entities needed by the mapping;
- active versions with the generated dump's VIDs and lower ranges;
- per-table max generated VIDs;
- the latest legacy POI digest;
- no prior changes, processed-block map, or immutable rows.

Never replace it with a fresh Graph Node SQL snapshot after generation starts.
Source VIDs and generated VIDs are different allocation domains; mixing them
causes clamps to target the wrong row.

## 4. Certify before any cutover

Restore the completed working dump with unmodified Graph Node v0.44 into a
disposable schema using the original deployment ID, then require:

- exact deployment/head/hash/graft metadata and healthy GraphQL `_meta`;
- zero-difference physical rows, temporal ranges, immutable blocks, and POI;
- representative GraphQL query equality;
- next-block processing, restart, and fork-revert recovery;
- repeatable restore and rollback rehearsal;
- sustained cached throughput that gains on Base head.

Production replacement remains a separate, explicitly approved operation. The
writer must be stopped, the old schema recoverable, and query workers detached
until the restored metadata and entity/POI state are certified atomically.

## 5. Rehearse replacement and rollback

On the disposable database, use Graph Node v0.44's native replacement path:

```bash
RESTORE_MODE=replace ./oracle/restore-native-fixture.sh \
  /backfill/working-dump \
  oracle/native-parquet-fixture-1
```

Run `verify-restored-parity.sh` followed by
`verify-restored-lifecycle.sh`. Preserve both the pre-replacement database
snapshot and immutable native dump. Rehearse rollback by stopping the sole
writer and restoring the preserved dump with `graphman restore --replace`, then
require the same metadata, GraphQL, POI, and physical gates before reopening
query traffic.

For a production cutover, first record an explicit approval, exact target
shard/schema, stopped writer and query assignments, backup identifiers, and
rollback owner. There must never be a window where Graph Node and the external
backfill writer can both advance the deployment. This repository intentionally
does not automate that production mutation.
