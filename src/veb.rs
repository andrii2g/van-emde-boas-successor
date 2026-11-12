use std::{error::Error, fmt};

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
        self.root.min
    }
    pub fn maximum(&self) -> Option<u32> {
        self.root.max
    }

    /// Returns false if the key was already present.
    pub fn insert(&mut self, key: u32) -> Result<bool, KeyError> {
        self.check_key(key)?;
        if self.root.contains(key) {
            return Ok(false);
        }
        self.root.insert(key);
        self.len += 1;
        Ok(true)
    }

    /// Returns false if the key was absent.
    pub fn remove(&mut self, key: u32) -> Result<bool, KeyError> {
        self.check_key(key)?;
        if !self.root.contains(key) {
            return Ok(false);
        }
        self.root.remove(key);
        self.len -= 1;
        Ok(true)
    }

    pub fn contains(&self, key: u32) -> Result<bool, KeyError> {
        self.check_key(key)?;
        Ok(self.root.contains(key))
    }

    /// Largest stored key strictly less than `key`.
    pub fn predecessor(&self, key: u32) -> Result<Option<u32>, KeyError> {
        self.check_key(key)?;
        Ok(self.root.predecessor(key))
    }

    /// Smallest stored key strictly greater than `key`.
    pub fn successor(&self, key: u32) -> Result<Option<u32>, KeyError> {
        self.check_key(key)?;
        Ok(self.root.successor(key))
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
        let count = self.root.validate("root")?;
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

    fn contains(&self, key: u32) -> bool {
        if self.min == Some(key) || self.max == Some(key) {
            return true;
        }
        if self.min.is_none() || self.universe == 2 {
            return false;
        }
        self.clusters[self.high(key)].contains(self.low(key))
    }

    // Precondition: key is absent and within the local universe.
    fn insert(&mut self, mut key: u32) {
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
            if self.clusters[high].min.is_none() {
                self.summary.as_mut().unwrap().insert(high as u32);
            }
            self.clusters[high].insert(low);
        }
        self.max = Some(self.max.unwrap().max(key));
    }

    // Precondition: key exists. The public wrapper checks this explicitly.
    fn remove(&mut self, mut key: u32) {
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
            let first = self.summary.as_ref().unwrap().min.unwrap() as usize;
            key = self.index(first, self.clusters[first].min.unwrap());
            self.min = Some(key);
        }
        let high = self.high(key);
        let low = self.low(key);
        self.clusters[high].remove(low);
        if self.clusters[high].min.is_none() {
            self.summary.as_mut().unwrap().remove(high as u32);
        }
        if self.max == Some(key) {
            self.max = match self.summary.as_ref().unwrap().max {
                None => self.min,
                Some(last) => {
                    Some(self.index(last as usize, self.clusters[last as usize].max.unwrap()))
                }
            };
        }
    }

    fn successor(&self, key: u32) -> Option<u32> {
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
        if self.clusters[high].max.is_some_and(|max| low < max) {
            let next = self.clusters[high].successor(low).unwrap();
            Some(self.index(high, next))
        } else {
            let next = self.summary.as_ref().unwrap().successor(high as u32)? as usize;
            Some(self.index(next, self.clusters[next].min.unwrap()))
        }
    }

    fn predecessor(&self, key: u32) -> Option<u32> {
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
        if self.clusters[high].min.is_some_and(|min| low > min) {
            let previous = self.clusters[high].predecessor(low).unwrap();
            Some(self.index(high, previous))
        } else {
            match self.summary.as_ref().unwrap().predecessor(high as u32) {
                Some(previous) => Some(self.index(
                    previous as usize,
                    self.clusters[previous as usize].max.unwrap(),
                )),
                None => self.min,
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

    fn validate(&self, path: &str) -> Result<usize, String> {
        let fail = |message: &str| format!("{path} (U={}): {message}", self.universe);
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
        summary.validate(&format!("{path}/summary"))?;
        let mut count = usize::from(self.min.is_some());
        let mut actual_max = self.min;
        for (i, cluster) in self.clusters.iter().enumerate() {
            count += cluster.validate(&format!("{path}/cluster[{i}]"))?;
            if summary.contains(i as u32) != cluster.min.is_some() {
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
