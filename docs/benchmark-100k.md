# Base giant 100k-block benchmark

Date: 2026-08-03

Deployment: `Qmbsc6XQWbiv4DfLVfaNciScqYLyDWUYjWzrFBbzzmRsMB`

## Decision

**Proceed to production hardening.** Cached Substreams replay, deterministic
Rust reduction, Graph Node-native Parquet generation, and one Graph Node v0.44
restore all gain substantially on Base's measured 0.5 block/s arrival rate.
The historical path does not need SQL Sink or parallel PostgreSQL writers.

This is an offline certification result, not authorization to replace live
`sgd1246`. Production still requires the backup, writer-exclusion, target-head,
query-detachment, and rollback approvals in
[production-backfill.md](production-backfill.md).

## Pinned range and density

The fixture is [benchmark-range.json](../fixtures/benchmark-range.json):

- graft seed: block 26,990,278,
  `0x3365afb2ec41d64886321de82b8fd73427fda55fcf731bf5a2888c1a2a2c8948`;
- segment 1: blocks 26,990,279–27,040,278;
- segment 2: blocks 27,040,279–27,090,278;
- total requested range: 100,000 blocks.

The map emitted 87,559 mapping events in 46,063 event-bearing blocks:
0.8756 events per requested block and 46.06% event-bearing blocks. The two
segments contained 38,709 and 48,850 events respectively. Cold and cached
JSONL were byte-identical at 86,905,399 bytes with SHA-256
`1af462c04c1e4ac46d00f2f5600d2f15b025687562d6a4f4ca4c83a38fbdc3f5`.

## Environment

- Apple M1 Max, 10 CPU cores, 64 GiB RAM;
- macOS 26.5.1, arm64;
- Rust/Cargo 1.90.0;
- Substreams CLI development build `724dbd4` dated 2026-04-03;
- Docker 29.2.1;
- Graph Node v0.44.0 image digest
  `sha256:c14c6d8e2b2b1ed89f6a89babf48d807b18a43e372fda7fb495d3a9050b65b8b`;
- PostgreSQL 16.11 in the disposable oracle;
- Pinax Base Substreams/Firehose and archive RPC endpoints.

Graph Node ran as its pinned linux/amd64 image under arm64 emulation, so its
local continuation result is conservative relative to a native production
host. Production `sgd1246` was read-only and its writer deployment remained at
zero replicas for every comparison.

## Stage results

| Stage | Wall time | Rate | Client CPU | Peak RSS | Output |
|---|---:|---:|---:|---:|---:|
| Active graft-state capture | 583.31 s | 404 active rows/s | 14% | 2.35 GB | 235,772 rows, 126 pages, 312,745,547-byte checkpoint |
| Substreams cold computation | 127.66 s | 783 requested blocks/s; 686 events/s | 4% | 87 MB | 86,905,399 bytes |
| Fully cached replay | 20.79 s | 4,810 requested blocks/s; 4,212 events/s | 20% | 87 MB | byte-identical |
| Segment 1 reducer | 19.26 s | 2,596 blocks/s | 96% | 4.98 GB | 323,650,305-byte checkpoint |
| Segment 1 Parquet write | 186.99 s | 267 blocks/s; 4,000 records/s | 99% | 5.87 GB | seed plus segment 1 |
| Segment 2 reducer | 22.05 s | 2,268 blocks/s | 99% | 6.14 GB | 335,658,023-byte checkpoint |
| Segment 2 Parquet append | 243.52 s | 205 blocks/s; 1,686 records/s | 99% | 7.25 GB | final artifact |
| Graph Node `restore --replace` plus sequence audit | 146.50 s | 7,886 inserted rows/s | 0.3% wrapper only | 34 MB wrapper RSS | 1,155,270 rows |

Client CPU is `(user + system) / wall` from macOS `time -l`. Substreams service
CPU and Graph Node/PostgreSQL container CPU were not exposed as a comparable
per-process total, so the table does not invent those values. The reducer and
Parquet stages are local and show that Parquet materialization saturates one
core; the Substreams rows show only local CLI overhead.

The final artifact contains 1,155,270 entity-version rows plus 3,223 clamp
records in 44 chunk and 22 clamp files. Its Parquet payload is 146,147,282
bytes. The restored `sgd13` tables occupy 1,596,686,336 bytes and GraphQL serves
the original deployment identity at block 27,090,278 without indexing errors.

Parquet materialization is the dominant steady-state stage. It averaged 2,691
chunk/clamp records/s and 0.339 MB/s while holding one CPU core near 100%; it
does not benefit from adding parallel SQL writers. The two reducers produced
1,818,909 ordered entity changes from the cached event stream.

Excluding the one-time graft capture and final restore, the corrected pipeline
processed the 100k range in:

- 599.48 s cold, or 166.8 blocks/s;
- 492.61 s cached, or 203.0 blocks/s.

Including the single restore, those rates are 134.1 and 156.5 blocks/s. The
first complete rehearsal, including the one-time active-state capture, was
75.2 blocks/s.

## Restart and transport behavior

The append journal was deliberately stopped after seven tables. All completed
chunk/clamp hashes validated, metadata remained at the prior checkpoint, and a
second invocation completed without rewriting journaled tables. The planned
stop cost 237.56 s and the successful resume cost 240.71 s; a normal uninterrupted
second append costs 243.52 s.

Two production-sized transport failures were found and fixed during the run:

- active mutable state is now keyset-paged by VID, capped at 2,000 rows by
  default, with a count/last-VID manifest and three retries per page;
- restored physical comparison is keyset-paged and gzip-verified instead of
  trusting one multi-gigabyte Kubernetes exec stream.

The paged seed capture recovered from one deliberately observed incomplete
`PoolHourData` page and still certified all 235,772 active rows.

## Correctness gates

Three independent logical/POI checkpoints passed:

| Checkpoint | Range | Compared entities | Result |
|---|---:|---:|---|
| First | 26,990,279–26,990,520 | 798 | zero mismatches; POI match |
| Decimal boundary | 27,071,830–27,072,071 | 2,029 | zero mismatches; POI match |
| Final | 27,090,037–27,090,278 | 681 | zero mismatches; POI match |

The 100k physical gate initially found values that differed by one or two units
in the 31st decimal place. Graph Node v0.44 normalizes `bigdecimal` 0.1.2 once
inside a host arithmetic operation and again when the returned value crosses
the WASM ABI. That second normalization is observable for some negative
34-digit boundaries. The reducer now reproduces the ABI round-trip; the exact
production regression value, its entity checkpoint, and POI all pass.

The exhaustive physical comparison transferred the complete active seed and
all history produced in the 100k range. All 20 tables and
1,155,270/1,155,270 canonical rows matched with zero differences. It completed
in 931.79 seconds using bounded, gzip-verified pages and 252 MB peak client RSS.
This includes 46,064 legacy POI rows; `vid` alone is excluded because Graph
Node's own v0.44 restore renumbers legacy dump rows. The replacement wrapper
separately reconciled and audited every resulting table sequence against its
maximum VID before Graph Node started.

## Graph Node continuation

The restored deployment then ran its unmodified v0.44 WASM writer from block
27,090,279. Two deterministic replay runs reached the requested 1,000-block
target in 32 and 36 seconds, or 27.8–31.25 requested blocks/s. Graph Node
committed normal batches beyond the target, after which the harness
force-rewound to the exact target block
27,091,278 and hash
`0xc61727c87bb1337c0d77337d8d7130bdf8ce421fc1349c782e0abea88e707794`.
All 20 tables and 10,366/10,366 rows changed by the target range matched the
paused production oracle, including 515 POI rows. The target GraphQL snapshot
was byte-identical after restart (SHA-256
`c384e434c14119b8ddb5e9f53c12bbf649bc965c9fc20206d26a07a5c65a01fe`),
and rewind returned byte-identically to the Parquet seed (SHA-256
`3eb5b548a0135d2294f4bfc52cdcddb912b407af5d59e8358265336223ce495c`).

The first continuation attempt exposed a `restore --replace` sequence hazard:
the replacement schema could retain a per-table sequence behind the rows that
Graph Node had renumbered, causing a duplicate `vid` on the first normal write.
The restore wrapper now reconciles every sequence while Graph Node is stopped,
audits the result, and refuses startup on any discrepancy. A fresh full restore
plus the continuation/restart/rewind sequence above proves the fix.

The rollback rehearsal advanced the disposable deployment to the continuation
target, then restored the preserved 100k Parquet checkpoint with
`restore --replace`. Graph Node returned to the exact seed head/hash with the
same GraphQL checksum
`3eb5b548a0135d2294f4bfc52cdcddb912b407af5d59e8358265336223ce495c`,
healthy status, and all 20 VID sequences
audited successfully in the new replacement schema.

Graph Node unregisters and recreates some deployment Prometheus counters across
these large commits. The harness marks a metric sample incomplete when its
processed-block counter does not cover the requested range, and does not derive
a trigger rate from that partial sample. The wall-clock target rate and exact
checkpoint hashes remain independently measured.

A previous matched production locality benchmark processed adjacent warm
windows at 0.763–0.816 blocks/s and 135.7–138.7 triggers/s; moving Firehose
locally did not improve it. Even the restored continuation's slower 27.8
blocks/s replay is well above Base's observed 0.5 blocks/s arrival rate, so
Graph Node can maintain
the live tail after the historical handoff on this sample. This benchmark uses
the same trigger-normalized interpretation rather than comparing Graph Node
with download-only Firehose speed.

## Projection

At 2026-08-03 03:41 UTC:

- paused production head: 32,374,154;
- Base head: 49,469,584;
- current gap: 17,095,430 blocks;
- measured Base arrival: 0.5 blocks/s over a 244-second sample.

Allowing for the advancing head, cached preprocess/reduce/Parquet work projects
to about 23.5 hours from the paused head to live, or 30.8 hours from the graft
block to live. The corresponding cold projections are 28.6 and 37.6 hours.
These are straight-line estimates from one 100k sample; production must retain
segment checkpoints because event density varies.

Incremental Parquet projects to approximately 7.9 GB from graft to the paused
head or 32.9 GB from graft to the sampled live head. This is additive to the
roughly 250 GB native graft dump that preserves pre-graft history. The active
reducer checkpoint grew from 312.7 MB to 323.7 MB and then 335.7 MB across the
two segments.

No cloud-price claim is made from a local benchmark. The cost envelope is one
historical Substreams job, one single-core Parquet materializer with an 8 GB
peak, native dump storage plus the projected increment, and one bulk Graph Node
restore—without a parallel SQL sink fleet.

## Reproduction

Use the production procedure and scripts rather than copying temporary paths:

```bash
./scripts/capture-active-seed.sh /backfill/graft-dump /backfill/checkpoint-26990278.json

./scripts/append-parquet-segment.sh \
  /backfill/working-dump /backfill/checkpoint-26990278.json \
  26990279 27040278 /backfill/checkpoint-27040278.json

./scripts/append-parquet-segment.sh \
  /backfill/working-dump /backfill/checkpoint-27040278.json \
  27040279 27090278 /backfill/checkpoint-27090278.json

RESTORE_MODE=replace ./oracle/restore-native-fixture.sh \
  /backfill/working-dump oracle/existing-name
```

Run `verify-state-parity.sh`, `verify-restored-parity.sh`, and
`oracle/benchmark-continuation.sh` against the disposable restored schema before
requesting a production cutover.

The exact continuation invocation for this fixture was:

```bash
CONTINUATION_REPORT=/tmp/v4-continuation.json \
  ./oracle/benchmark-continuation.sh \
    sgd13 oracle/native-parquet-resume-script-1 \
    27090278 0x2e8452649a968006c58e7530d5e1ac054ae1f30383484d4736cea0edfd84b8ad \
    27091278 0xc61727c87bb1337c0d77337d8d7130bdf8ce421fc1349c782e0abea88e707794

PARITY_REPORT=/tmp/v4-continuation-physical.json \
SEED_BLOCK=27090278 START_BLOCK=27090279 END_BLOCK=27091278 \
SEED_ID_SCOPE=changed ./scripts/verify-restored-parity.sh sgd13
```
