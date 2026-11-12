use std::cell::Cell;

/// Order of the public operation counters.
pub const OPERATION_NAMES: [&str; 7] = [
    "insert",
    "remove",
    "contains",
    "predecessor",
    "successor",
    "minimum",
    "maximum",
];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub operations: [u64; 7],
    pub node_visits: u64,
    /// Node entries below the root, including internal membership prechecks.
    pub recursive_calls: u64,
    pub cluster_accesses: u64,
    pub summary_accesses: u64,
    /// Root is depth zero.
    pub max_depth: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Storage {
    pub allocated_nodes: usize,
    /// Includes summary nodes as well as data clusters.
    pub nonempty_nodes: usize,
    pub heap_allocations: usize,
    /// Struct plus owned node buffers; excludes allocator metadata and rounding.
    pub owned_bytes: usize,
}

#[derive(Debug)]
pub(crate) struct Counters {
    pub(crate) enabled: bool,
    pub(crate) values: Cell<Metrics>,
}
impl Default for Counters {
    fn default() -> Self {
        Self {
            enabled: true,
            values: Cell::default(),
        }
    }
}
impl Counters {
    pub(crate) fn operation(&self, kind: usize) {
        if self.enabled {
            let mut values = self.values.get();
            values.operations[kind] += 1;
            self.values.set(values);
        }
    }
}

// Const specialization removes all recursive counter work in disabled runs.
#[derive(Clone, Copy)]
pub(crate) struct Probe<'a, const COUNT: bool> {
    counters: &'a Counters,
    depth: u32,
}
impl<'a, const COUNT: bool> Probe<'a, COUNT> {
    pub(crate) fn new(counters: &'a Counters) -> Self {
        Self { counters, depth: 0 }
    }
    pub(crate) fn child(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }
    pub(crate) fn visit(self) {
        if COUNT {
            let mut values = self.counters.values.get();
            values.node_visits += 1;
            values.recursive_calls += u64::from(self.depth > 0);
            values.max_depth = values.max_depth.max(self.depth);
            self.counters.values.set(values);
        }
    }
    pub(crate) fn cluster(self) {
        if COUNT {
            let mut values = self.counters.values.get();
            values.cluster_accesses += 1;
            self.counters.values.set(values);
        }
    }
    pub(crate) fn summary(self) {
        if COUNT {
            let mut values = self.counters.values.get();
            values.summary_accesses += 1;
            self.counters.values.set(values);
        }
    }
}
