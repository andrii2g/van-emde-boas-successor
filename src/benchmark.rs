//! Identical pre-generated traces, untimed exact verification, then alternating timed runs.

use crate::{
    Metrics, OPERATION_NAMES, Storage, VebSet,
    workload::{self, Generator, Operation, Outcome},
};
use std::{collections::BTreeSet, hint::black_box, time::Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workload {
    Queries,
    Mixed,
}
impl Workload {
    pub fn name(self) -> &'static str {
        match self {
            Self::Queries => "queries",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub universe: u32,
    pub operations: usize,
    pub seed: u64,
    /// Initial occupancy, in percent. Mixed workloads may change it.
    pub occupancy: u32,
    pub repeats: usize,
    pub workload: Workload,
}

#[derive(Debug)]
pub struct Report {
    pub config: Config,
    pub initial_len: usize,
    pub final_len: usize,
    pub metrics: Metrics,
    pub storage: Storage,
    pub veb_setup_ns: Vec<u128>,
    pub btree_setup_ns: Vec<u128>,
    pub veb_batch_ns: Vec<u128>,
    pub btree_batch_ns: Vec<u128>,
}

fn initial_keys(config: Config) -> Vec<u32> {
    let mut keys: Vec<_> = (0..config.universe).collect();
    let mut rng = Generator(config.seed ^ 0xd1b54a32d192ed03);
    for i in (1..keys.len()).rev() {
        let other = (rng.next() % (i as u64 + 1)) as usize;
        keys.swap(i, other);
    }
    keys.truncate((u64::from(config.universe) * u64::from(config.occupancy) / 100) as usize);
    keys
}

pub fn run(config: Config) -> Result<Report, String> {
    if config.operations == 0 || config.repeats == 0 || config.occupancy > 100 {
        return Err("operations/repeats must be positive and occupancy must be in 0..=100".into());
    }
    let mut operations = workload::generate(config.universe, config.operations, config.seed)
        .map_err(|e| e.to_string())?;
    if config.workload == Workload::Queries {
        for operation in &mut operations {
            *operation = match *operation {
                Operation::Insert(key) => Operation::Contains(key),
                Operation::Remove(key) => Operation::Successor(key),
                other => other,
            };
        }
    }
    let initial = initial_keys(config);
    // Separate instrumented replay: compare every result, then final contents and invariants.
    let mut measured = VebSet::new(config.universe).map_err(|e| e.to_string())?;
    let mut oracle = BTreeSet::new();
    for &key in &initial {
        measured.insert(key).unwrap();
        oracle.insert(key);
    }
    measured.reset_metrics();
    for (index, &operation) in operations.iter().enumerate() {
        if operation.apply_veb(&mut measured).unwrap() != operation.apply_btree(&mut oracle) {
            return Err(format!(
                "benchmark preflight mismatch at operation[{index}]={operation:?}, config={config:?}"
            ));
        }
    }
    measured.validate()?;
    let expected: Vec<_> = oracle.iter().copied().collect();
    if measured.to_vec() != expected {
        return Err("benchmark preflight contents differ".into());
    }
    let mut report = Report {
        config,
        initial_len: initial.len(),
        final_len: expected.len(),
        metrics: measured.metrics(),
        storage: measured.storage(),
        veb_setup_ns: Vec::new(),
        btree_setup_ns: Vec::new(),
        veb_batch_ns: Vec::new(),
        btree_batch_ns: Vec::new(),
    };
    drop(measured);
    drop(oracle);
    // One warmup per implementation, followed by measured repetitions with alternating order.
    for iteration in 0..=config.repeats {
        let (veb, btree) = if iteration % 2 == 0 {
            let v = timed_veb(config.universe, &initial, &operations, &expected)?;
            let b = timed_btree(&initial, &operations, &expected)?;
            (v, b)
        } else {
            let b = timed_btree(&initial, &operations, &expected)?;
            let v = timed_veb(config.universe, &initial, &operations, &expected)?;
            (v, b)
        };
        if veb.2 != btree.2 {
            return Err("timed replay checksums differ".into());
        }
        if iteration > 0 {
            report.veb_setup_ns.push(veb.0);
            report.veb_batch_ns.push(veb.1);
            report.btree_setup_ns.push(btree.0);
            report.btree_batch_ns.push(btree.1);
        }
    }
    Ok(report)
}

fn digest(current: u64, outcome: Outcome) -> u64 {
    let value = match outcome {
        Outcome::Changed(value) => u64::from(value) + 1,
        Outcome::Present(value) => u64::from(value) + 3,
        Outcome::Key(None) => 5,
        Outcome::Key(Some(key)) => u64::from(key) + 6,
    };
    current.rotate_left(7) ^ value
}

fn timed_veb(
    universe: u32,
    initial: &[u32],
    operations: &[Operation],
    expected: &[u32],
) -> Result<(u128, u128, u64), String> {
    let start = Instant::now();
    let mut set = VebSet::new(universe).map_err(|e| e.to_string())?;
    set.set_metrics_enabled(false);
    for &key in initial {
        set.insert(key).unwrap();
    }
    let setup = start.elapsed().as_nanos();
    let mut checksum = 0;
    let start = Instant::now();
    for &operation in black_box(operations) {
        checksum = digest(checksum, operation.apply_veb(black_box(&mut set)).unwrap());
    }
    black_box(checksum);
    let elapsed = start.elapsed().as_nanos();
    if set.to_vec() != expected {
        return Err("timed vEB contents differ".into());
    }
    Ok((setup, elapsed, checksum))
}

fn timed_btree(
    initial: &[u32],
    operations: &[Operation],
    expected: &[u32],
) -> Result<(u128, u128, u64), String> {
    let start = Instant::now();
    let mut set = BTreeSet::new();
    for &key in initial {
        set.insert(key);
    }
    let setup = start.elapsed().as_nanos();
    let mut checksum = 0;
    let start = Instant::now();
    for &operation in black_box(operations) {
        checksum = digest(checksum, operation.apply_btree(black_box(&mut set)));
    }
    black_box(checksum);
    let elapsed = start.elapsed().as_nanos();
    if !set.iter().copied().eq(expected.iter().copied()) {
        return Err("timed BTreeSet contents differ".into());
    }
    Ok((setup, elapsed, checksum))
}

fn median(samples: &[u128]) -> u128 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    // Upper median for an even number of samples; no interpolated timing.
    sorted[sorted.len() / 2]
}

pub fn csv_header() -> String {
    format!(
        "implementation,universe,seed,workload,occupancy_percent,initial_len,final_len,operations,repeats,os,arch,profile,setup_median_ns,batch_min_ns,batch_median_ns,batch_max_ns,median_ns_per_op,node_visits,recursive_calls,cluster_accesses,summary_accesses,max_depth,allocated_nodes,nonempty_nodes,heap_allocations,owned_bytes,{}\n",
        OPERATION_NAMES.join(",")
    )
}

impl Report {
    pub fn csv_rows(&self) -> String {
        let mut csv = String::new();
        for (name, setup, batch, instrumented) in [
            ("veb", &self.veb_setup_ns, &self.veb_batch_ns, true),
            ("btree", &self.btree_setup_ns, &self.btree_batch_ns, false),
        ] {
            let c = self.config;
            let mut fields = vec![
                name.to_string(),
                c.universe.to_string(),
                c.seed.to_string(),
                c.workload.name().to_string(),
                c.occupancy.to_string(),
                self.initial_len.to_string(),
                self.final_len.to_string(),
                c.operations.to_string(),
                c.repeats.to_string(),
                std::env::consts::OS.into(),
                std::env::consts::ARCH.into(),
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                }
                .into(),
                median(setup).to_string(),
                batch.iter().min().unwrap().to_string(),
                median(batch).to_string(),
                batch.iter().max().unwrap().to_string(),
                format!("{:.2}", median(batch) as f64 / c.operations as f64),
            ];
            if instrumented {
                let m = self.metrics;
                let s = self.storage;
                fields.extend([
                    m.node_visits.to_string(),
                    m.recursive_calls.to_string(),
                    m.cluster_accesses.to_string(),
                    m.summary_accesses.to_string(),
                    m.max_depth.to_string(),
                    s.allocated_nodes.to_string(),
                    s.nonempty_nodes.to_string(),
                    s.heap_allocations.to_string(),
                    s.owned_bytes.to_string(),
                ]);
            } else {
                fields.extend(std::iter::repeat_n(String::new(), 9));
            }
            // Public call counts are identical because both implementations replay the same trace.
            fields.extend(self.metrics.operations.iter().map(u64::to_string));
            csv.push_str(&fields.join(","));
            csv.push('\n');
        }
        csv
    }
}
