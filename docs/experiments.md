# Reproducible experiments

## Recorded run

Recorded on 2026-11-12 using Rust 1.97.1 (`8bab26f4f`, LLVM 22.1.6), target
`x86_64-pc-windows-msvc`, release profile with Cargo defaults. These are local
wall-clock measurements on a shared workstation, with no CPU pinning or
frequency controls. They are example observations, not performance guarantees.

```sh
cargo run --release -- bench --operations 10000 --repeats 7 --workload queries --output docs/results/queries.csv
cargo run --release -- bench --operations 10000 --repeats 7 --workload mixed --output docs/results/mixed.csv
```

Committed snapshots: [query results](results/queries.csv),
[mixed-operation results](results/mixed.csv). The traces and counters are
deterministic for a seed; elapsed times will vary between runs.

At U=65536:

| Workload | Initial occupancy | vEB median ns/op | BTreeSet median ns/op |
|---|---:|---:|---:|
| Queries | 1% | 18.71 | 26.91 |
| Queries | 50% | 31.66 | 45.67 |
| Queries | 90% | 32.43 | 48.03 |
| Mixed | 1% | 29.07 | 36.13 |
| Mixed | 50% | 64.49 | 50.12 |
| Mixed | 90% | 39.83 | 54.49 |

vEB used 4,416,448 owned bytes and 92,007 nodes for every one of these states.
The 50%-occupancy mixed result also illustrates why timing variation and
workload must be visible. Small batches, cache state, branch behavior, the
allocator, and OS scheduling can influence these figures substantially.

## Trace generation

SplitMix64 uses fixed constants and wrapping 64-bit arithmetic. Golden-value
tests anchor the generator. For mixed operations the nominal probabilities
are 20% insert, 20% remove, 10% contains, 20% predecessor, 20% successor, and
10% extrema queries. Minimum vs maximum uses the operation index parity.
Three out of each 16 positions choose boundary keys (0, U-1, U/2); others use
pseudorandom keys. Counts are recorded rather than assuming exact proportions.

Read-only query traces replace inserts with membership queries and removes
with successor queries. No update occurs during their timed batch. Mixed
traces can change cardinality; occupancy labels always refer to initialization,
and CSV reports both initial and final length.

Initial keys come from a deterministically shuffled universe, using a separate
seed stream. Cardinality is `floor(U * occupancy_percent / 100)`. In particular,
1% of U=16 rounds down to an empty set; inspect `initial_len` rather than
assuming every requested percentage is representable. Every repetition starts
from the same shuffled prefix and replays the same operation order.

## Timing protocol

1. Generate keys and the operation vector before timing.
2. Replay once with vEB metrics enabled and compare **each exact result** to
   `BTreeSet`. Validate final invariants and compare all sorted contents.
3. Drop those preflight sets. Warm up both implementations once.
4. Create fresh sets for every sample. Measure setup separately: empty
   construction plus insertion of the same initial keys in the same order.
5. Time the operation batch with vEB counters disabled. Both loops include
   operation dispatch, a rolling outcome checksum, and `black_box` barriers.
6. Stop timing before final-content verification or destruction. Verify both
   final sets and compare the timed-run checksums.
7. Alternate implementation order between repetitions. Report batch min,
   median, and max, plus setup median. An even sample count uses the upper
   median. `median_ns_per_op` is batch median divided by operation count,
   not the median of individually timed operations.

The checksum protects timed outputs from elimination; it does not replace the
untimed exact oracle checks. Workload generation, full validation, SVG rendering,
storage traversal, deallocation, and CSV formatting are outside timed batches.
The vEB public API still dispatches between counter-enabled/disabled code at
root routine entries, but disabled recursive specializations contain no counter updates.
The driver and Rust API shapes are not instruction-for-instruction identical;
these figures compare the implemented public sets under the same workload.

## Counter and memory definitions

- `operations` is the number of requested public calls in a batch. The final
  seven CSV columns split it into insert/remove/contains/predecessor/successor/
  minimum/maximum counts. Both implementations receive the same calls.
- `node_visits` counts internal algorithm entries, including root entries and
  insert/remove membership prechecks. Cached extrema queries do not enter a
  recursive node routine.
- `recursive_calls` counts entries strictly below the root. It includes
  precheck traversals, and both cluster and summary recursion.
- `cluster_accesses` counts indexed cluster-reference acquisitions in the core
  algorithms, including reads of extrema. Re-reading a cluster counts again.
  It is not a hardware cache or memory-load counter.
- `summary_accesses` counts summary-reference acquisitions, including cached
  extrema reads and recursive calls.
- `max_depth` is the largest number of edges below the root reached by a call.
- `allocated_nodes` includes root, all cluster nodes, and all summary nodes.
  `nonempty_nodes` includes summaries and describes logical occupancy after
  preflight. `heap_allocations` counts retained boxes/buffers, not node count
  or cumulative allocator calls during construction.
- `owned_bytes` includes `size_of::<VebSet>()`, all separately boxed summaries,
  and all cluster slice elements, without double-counting inline nodes. It
  excludes allocator overhead/rounding and process memory. Workload, rendering,
  and report allocations are also excluded. This is storage accounting rather
  than an RSS measurement. BTreeSet storage and internal work are unmeasured;
  their CSV cells are blank.

Counters are gathered in a separate preflight; setup operations are cleared
before counting. Inspection/rendering do not modify them. CSV runtime samples
and counters therefore describe the same trace in distinct executions.

## Verification and diagram reproduction

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --release
cargo run --release -- verify --universe all --operations 10000 --seed 42
cargo run --release -- render --universe 16 --keys 2,3,7,9,14 --depth 1 --successor 7 --output docs/images/overview.svg
cargo run --release -- render --universe 16 --keys 2,3,7,9,14 --show-empty --successor 7 --output docs/images/universe-16.svg
cargo run --release -- render --universe 65536 --keys 2,3,7,255,256,257,4095,4096,65535 --depth 2 --successor 257 --output docs/images/universe-65536.svg
```

The three SVGs were parsed as XML; the complete small-universe SVG was also
visually checked in a headless Chromium rendering. Rendering requires no
external program; that browser was used only for visual QA. The suite also
checks deterministic output, diagram limits, domain labels, trace answers,
unchanged counters, invalid options, and CLI file generation.
