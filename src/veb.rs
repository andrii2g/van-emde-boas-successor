use crate::metrics::{Counters, Metrics, Probe, Storage};
use std::{error::Error, fmt};

macro_rules! dispatch {
    ($set:ident, $method:ident, $key:expr) => {
        if $set.counters.enabled {
            $set.root.$method($key, Probe::<true>::new(&$set.counters))
        } else {
            $set.root.$method($key, Probe::<false>::new(&$set.counters))
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniverseError(pub u32);

impl fmt::Display for UniverseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported universe {}; choose 16, 256, or 65536",
            self.0
        )
    }
}
impl Error for UniverseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyError {
    pub key: u32,
    pub universe: u32,
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "key {} is outside 0..{}", self.key, self.universe)
    }
}
impl Error for KeyError {}

/// An integer set with all recursive nodes allocated during construction.
#[derive(Debug)]
pub struct VebSet {
    pub(crate) root: Node,
    len: usize,
    counters: Counters,
}

#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) universe: u32,
    lower_bits: u32,
    pub(crate) min: Option<u32>,
    pub(crate) max: Option<u32>,
    pub(crate) summary: Option<Box<Node>>,
    pub(crate) clusters: Box<[Node]>,
}

impl VebSet {
    pub fn new(universe: u32) -> Result<Self, UniverseError> {
        if !matches!(universe, 16 | 256 | 65_536) {
            return Err(UniverseError(universe));
        }
        Ok(Self {
            root: Node::new(universe),
            len: 0,
            counters: Counters::default(),
        })
    }

    pub fn universe(&self) -> u32 {
        self.root.universe
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn minimum(&self) -> Option<u32> {
        self.counters.operation(5);
        self.root.min
    }
    pub fn maximum(&self) -> Option<u32> {
        self.counters.operation(6);
        self.root.max
    }

    /// Returns false if the key was already present.
    pub fn insert(&mut self, key: u32) -> Result<bool, KeyError> {
        self.counters.operation(0);
        self.check_key(key)?;
        if dispatch!(self, contains, key) {
            return Ok(false);
        }
        dispatch!(self, insert, key);
        self.len += 1;
        Ok(true)
    }

    /// Returns false if the key was absent.
    pub fn remove(&mut self, key: u32) -> Result<bool, KeyError> {
        self.counters.operation(1);
        self.check_key(key)?;
        if !dispatch!(self, contains, key) {
            return Ok(false);
        }
        dispatch!(self, remove, key);
        self.len -= 1;
        Ok(true)
    }

    pub fn contains(&self, key: u32) -> Result<bool, KeyError> {
        self.counters.operation(2);
        self.check_key(key)?;
        Ok(dispatch!(self, contains, key))
    }

    /// Largest stored key strictly less than `key`.
    pub fn predecessor(&self, key: u32) -> Result<Option<u32>, KeyError> {
        self.counters.operation(3);
        self.check_key(key)?;
        Ok(dispatch!(self, predecessor, key))
    }

    /// Smallest stored key strictly greater than `key`.
    pub fn successor(&self, key: u32) -> Result<Option<u32>, KeyError> {
        self.counters.operation(4);
        self.check_key(key)?;
        Ok(dispatch!(self, successor, key))
    }

    pub fn metrics(&self) -> Metrics {
        self.counters.values.get()
    }
    pub fn reset_metrics(&self) {
        self.counters.values.set(Metrics::default());
    }
    /// Disables recording without clearing existing counters.
    pub fn set_metrics_enabled(&mut self, enabled: bool) {
        self.counters.enabled = enabled;
    }
    /// Traverses owned storage without changing operation counters.
    pub fn storage(&self) -> Storage {
        let mut storage = Storage {
            owned_bytes: size_of::<Self>(),
            ..Storage::default()
        };
        self.root.storage(&mut storage);
        storage
    }

    fn check_key(&self, key: u32) -> Result<(), KeyError> {
        if key >= self.universe() {
            Err(KeyError {
                key,
                universe: self.universe(),
            })
        } else {
            Ok(())
        }
    }

    /// Returns all keys in order. Intended for inspection, not timed queries.
    pub fn to_vec(&self) -> Vec<u32> {
        let mut keys = Vec::with_capacity(self.len);
        self.root.collect(0, &mut keys);
        keys
    }

    /// Checks every node, summary membership, extrema, and the root length.
    /// This traverses the allocated hierarchy and is intentionally O(U).
    pub fn validate(&self) -> Result<(), String> {
        let count = self.root.validate()?;
        if count != self.len {
            return Err(format!("root length: expected {}, found {count}", self.len));
        }
        Ok(())
    }
}

impl Node {
    fn new(universe: u32) -> Self {
        let lower_bits = universe.trailing_zeros() / 2;
        let (summary, clusters) = if universe == 2 {
            (None, Vec::new().into_boxed_slice())
        } else {
            let side = 1 << lower_bits;
            (
                Some(Box::new(Self::new(side))),
                (0..side)
                    .map(|_| Self::new(side))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            )
        };
        Self {
            universe,
            lower_bits,
            min: None,
            max: None,
            summary,
            clusters,
        }
    }

    pub(crate) fn high(&self, key: u32) -> usize {
        (key >> self.lower_bits) as usize
    }
    pub(crate) fn low(&self, key: u32) -> u32 {
        key & ((1 << self.lower_bits) - 1)
    }
    pub(crate) fn index(&self, high: usize, low: u32) -> u32 {
        ((high as u32) << self.lower_bits) | low
    }

    fn contains<const COUNT: bool>(&self, key: u32, probe: Probe<'_, COUNT>) -> bool {
        probe.visit();
        if self.min == Some(key) || self.max == Some(key) {
            return true;
        }
        if self.min.is_none() || self.universe == 2 {
            return false;
        }
        self.cluster(self.high(key), probe)
            .contains(self.low(key), probe.child())
    }

    // Precondition: key is absent and within the local universe.
    fn insert<const COUNT: bool>(&mut self, mut key: u32, probe: Probe<'_, COUNT>) {
        probe.visit();
        let Some(min) = self.min else {
            self.min = Some(key);
            self.max = Some(key);
            return;
        };
        if key < min {
            std::mem::swap(&mut key, self.min.as_mut().unwrap());
        }
        if self.universe > 2 {
            let high = self.high(key);
            let low = self.low(key);
            if self.cluster(high, probe).min.is_none() {
                self.summary_mut(probe).insert(high as u32, probe.child());
            }
            self.cluster_mut(high, probe).insert(low, probe.child());
        }
        self.max = Some(self.max.unwrap().max(key));
    }

    // Precondition: key exists. The public wrapper checks this explicitly.
    fn remove<const COUNT: bool>(&mut self, mut key: u32, probe: Probe<'_, COUNT>) {
        probe.visit();
        if self.min == self.max {
            self.min = None;
            self.max = None;
            return;
        }
        if self.universe == 2 {
            let remaining = 1 - key;
            self.min = Some(remaining);
            self.max = Some(remaining);
            return;
        }
        if self.min == Some(key) {
            let first = self.summary(probe).min.unwrap() as usize;
            key = self.index(first, self.cluster(first, probe).min.unwrap());
            self.min = Some(key);
        }
        let high = self.high(key);
        let low = self.low(key);
        self.cluster_mut(high, probe).remove(low, probe.child());
        if self.cluster(high, probe).min.is_none() {
            self.summary_mut(probe).remove(high as u32, probe.child());
        }
        if self.max == Some(key) {
            self.max = match self.summary(probe).max {
                None => self.min,
                Some(last) => Some(self.index(
                    last as usize,
                    self.cluster(last as usize, probe).max.unwrap(),
                )),
            };
        }
    }

    fn successor<const COUNT: bool>(&self, key: u32, probe: Probe<'_, COUNT>) -> Option<u32> {
        probe.visit();
        let min = self.min?;
        if key < min {
            return Some(min);
        }
        if key >= self.max.unwrap() {
            return None;
        }
        if self.universe == 2 {
            return self.max;
        }
        let high = self.high(key);
        let low = self.low(key);
        if self.cluster(high, probe).max.is_some_and(|max| low < max) {
            let next = self
                .cluster(high, probe)
                .successor(low, probe.child())
                .unwrap();
            Some(self.index(high, next))
        } else {
            let next = self.summary(probe).successor(high as u32, probe.child())? as usize;
            Some(self.index(next, self.cluster(next, probe).min.unwrap()))
        }
    }

    fn predecessor<const COUNT: bool>(&self, key: u32, probe: Probe<'_, COUNT>) -> Option<u32> {
        probe.visit();
        let max = self.max?;
        if key > max {
            return Some(max);
        }
        if key <= self.min.unwrap() {
            return None;
        }
        if self.universe == 2 {
            return self.min;
        }
        let high = self.high(key);
        let low = self.low(key);
        if self.cluster(high, probe).min.is_some_and(|min| low > min) {
            let previous = self
                .cluster(high, probe)
                .predecessor(low, probe.child())
                .unwrap();
            Some(self.index(high, previous))
        } else {
            match self.summary(probe).predecessor(high as u32, probe.child()) {
                Some(previous) => Some(self.index(
                    previous as usize,
                    self.cluster(previous as usize, probe).max.unwrap(),
                )),
                None => self.min,
            }
        }
    }

    fn cluster<const COUNT: bool>(&self, index: usize, probe: Probe<'_, COUNT>) -> &Self {
        probe.cluster();
        &self.clusters[index]
    }
    fn cluster_mut<const COUNT: bool>(
        &mut self,
        index: usize,
        probe: Probe<'_, COUNT>,
    ) -> &mut Self {
        probe.cluster();
        &mut self.clusters[index]
    }
    fn summary<const COUNT: bool>(&self, probe: Probe<'_, COUNT>) -> &Self {
        probe.summary();
        self.summary.as_ref().unwrap()
    }
    fn summary_mut<const COUNT: bool>(&mut self, probe: Probe<'_, COUNT>) -> &mut Self {
        probe.summary();
        self.summary.as_mut().unwrap()
    }

    fn storage(&self, storage: &mut Storage) {
        storage.allocated_nodes += 1;
        storage.nonempty_nodes += usize::from(self.min.is_some());
        if let Some(summary) = &self.summary {
            storage.heap_allocations += 2; // Box<Node> and nonempty Box<[Node]>.
            storage.owned_bytes += size_of::<Self>() + self.clusters.len() * size_of::<Self>();
            summary.storage(storage);
            for cluster in &self.clusters {
                cluster.storage(storage);
            }
        }
    }

    fn collect(&self, offset: u32, keys: &mut Vec<u32>) {
        if let Some(min) = self.min {
            keys.push(offset + min);
        }
        if self.universe == 2 {
            if self.max != self.min {
                keys.push(offset + self.max.unwrap());
            }
        } else {
            for (i, cluster) in self.clusters.iter().enumerate() {
                if cluster.min.is_some() {
                    cluster.collect(offset + self.index(i, 0), keys);
                }
            }
        }
    }

    fn validate(&self) -> Result<usize, String> {
        let fail = |message: &str| format!("U={}: {message}", self.universe);
        if self.min.is_some() != self.max.is_some() {
            return Err(fail("only one extremum exists"));
        }
        if let (Some(min), Some(max)) = (self.min, self.max)
            && (min > max || max >= self.universe)
        {
            return Err(fail("invalid extrema"));
        }
        if self.universe == 2 {
            return Ok(match (self.min, self.max) {
                (None, None) => 0,
                (a, b) if a == b => 1,
                _ => 2,
            });
        }
        let summary = self.summary.as_ref().unwrap();
        summary
            .validate()
            .map_err(|error| format!("summary/{error}"))?;
        let mut count = usize::from(self.min.is_some());
        let mut actual_max = self.min;
        for (i, cluster) in self.clusters.iter().enumerate() {
            count += cluster
                .validate()
                .map_err(|error| format!("cluster[{i}]/{error}"))?;
            if summary.contains(i as u32, Probe::<false>::new(&Counters::default()))
                != cluster.min.is_some()
            {
                return Err(fail("summary membership differs from cluster occupancy"));
            }
            if let Some(min) = cluster.min {
                if self.min.is_none_or(|parent| self.index(i, min) <= parent) {
                    return Err(fail(
                        "cluster key is not strictly above the external minimum",
                    ));
                }
                actual_max = Some(self.index(i, cluster.max.unwrap()));
            }
        }
        if actual_max != self.max {
            return Err(fail("maximum differs from stored keys"));
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_duplicates_and_deleting_extrema() {
        for universe in [16, 256, 65_536] {
            let mut set = VebSet::new(universe).unwrap();
            assert_eq!(set.minimum(), None);
            assert_eq!(set.successor(0), Ok(None));
            assert_eq!(set.predecessor(universe - 1), Ok(None));
            for key in [universe - 1, 0, 1, universe / 2] {
                assert_eq!(set.insert(key), Ok(true));
                assert_eq!(set.insert(key), Ok(false));
            }
            assert_eq!(set.to_vec(), [0, 1, universe / 2, universe - 1]);
            assert_eq!(set.successor(1), Ok(Some(universe / 2)));
            assert_eq!(set.predecessor(universe / 2), Ok(Some(1)));
            set.validate().unwrap();
            for key in [0, universe - 1, 1, universe / 2] {
                assert_eq!(set.remove(key), Ok(true));
                assert_eq!(set.remove(key), Ok(false));
                set.validate().unwrap();
            }
            assert!(set.is_empty());
            assert_eq!(set.maximum(), None);
        }
    }

    #[test]
    fn rejects_invalid_universes_and_keys() {
        for universe in [0, 2, 15, 17, u32::MAX] {
            assert!(VebSet::new(universe).is_err());
        }
        let mut set = VebSet::new(16).unwrap();
        for key in [16, u32::MAX] {
            assert!(set.insert(key).is_err());
            assert!(set.remove(key).is_err());
            assert!(set.contains(key).is_err());
            assert!(set.predecessor(key).is_err());
            assert!(set.successor(key).is_err());
        }
        assert!(set.is_empty());
    }
}
