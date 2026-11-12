//! Portable deterministic operations and an independent BTreeSet oracle.

use crate::{KeyError, UniverseError, VebSet};
use std::{
    collections::BTreeSet,
    ops::Bound::{Excluded, Unbounded},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Insert(u32),
    Remove(u32),
    Contains(u32),
    Predecessor(u32),
    Successor(u32),
    Minimum,
    Maximum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Changed(bool),
    Present(bool),
    Key(Option<u32>),
}

impl Operation {
    pub fn apply_veb(self, set: &mut VebSet) -> Result<Outcome, KeyError> {
        Ok(match self {
            Self::Insert(key) => Outcome::Changed(set.insert(key)?),
            Self::Remove(key) => Outcome::Changed(set.remove(key)?),
            Self::Contains(key) => Outcome::Present(set.contains(key)?),
            Self::Predecessor(key) => Outcome::Key(set.predecessor(key)?),
            Self::Successor(key) => Outcome::Key(set.successor(key)?),
            Self::Minimum => Outcome::Key(set.minimum()),
            Self::Maximum => Outcome::Key(set.maximum()),
        })
    }

    pub fn apply_btree(self, set: &mut BTreeSet<u32>) -> Outcome {
        match self {
            Self::Insert(key) => Outcome::Changed(set.insert(key)),
            Self::Remove(key) => Outcome::Changed(set.remove(&key)),
            Self::Contains(key) => Outcome::Present(set.contains(&key)),
            Self::Predecessor(key) => Outcome::Key(set.range(..key).next_back().copied()),
            Self::Successor(key) => {
                Outcome::Key(set.range((Excluded(key), Unbounded)).next().copied())
            }
            Self::Minimum => Outcome::Key(set.first().copied()),
            Self::Maximum => Outcome::Key(set.last().copied()),
        }
    }

    pub fn is_mutation(self) -> bool {
        matches!(self, Self::Insert(_) | Self::Remove(_))
    }
}

/// SplitMix64 with explicit wrapping arithmetic: identical sequences across platforms.
pub(crate) struct Generator(pub(crate) u64);
impl Generator {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }
}

pub fn generate(universe: u32, count: usize, seed: u64) -> Result<Vec<Operation>, UniverseError> {
    if !matches!(universe, 16 | 256 | 65_536) {
        return Err(UniverseError(universe));
    }
    let mut rng = Generator(seed);
    let mut operations = Vec::with_capacity(count);
    for index in 0..count {
        let key = match index % 16 {
            0 => 0,
            1 => universe - 1,
            2 => universe / 2,
            _ => (rng.next() % u64::from(universe)) as u32,
        };
        operations.push(match rng.next() % 10 {
            0..=1 => Operation::Insert(key),
            2..=3 => Operation::Remove(key),
            4 => Operation::Contains(key),
            5..=6 => Operation::Predecessor(key),
            7..=8 => Operation::Successor(key),
            9 if index % 2 == 0 => Operation::Minimum,
            _ => Operation::Maximum,
        });
    }
    Ok(operations)
}

/// Compares every result, and all contents/invariants after every attempted mutation.
/// Errors include a zero-based operation index and enough context to replay the prefix.
pub fn verify(universe: u32, count: usize, seed: u64) -> Result<usize, String> {
    let operations = generate(universe, count, seed).map_err(|error| error.to_string())?;
    let mut actual = VebSet::new(universe).map_err(|error| error.to_string())?;
    let mut expected = BTreeSet::new();
    for (index, operation) in operations.into_iter().enumerate() {
        let context = || format!("U={universe}, seed={seed}, operation[{index}]={operation:?}");
        let got = operation
            .apply_veb(&mut actual)
            .map_err(|error| format!("{}: {error}", context()))?;
        let want = operation.apply_btree(&mut expected);
        if got != want {
            return Err(format!("{}: expected {want:?}, got {got:?}", context()));
        }
        if operation.is_mutation() {
            actual
                .validate()
                .map_err(|error| format!("{}: invariant: {error}", context()))?;
            let contents: Vec<_> = expected.iter().copied().collect();
            if actual.len() != expected.len()
                || actual.to_vec() != contents
                || actual.minimum() != expected.first().copied()
                || actual.maximum() != expected.last().copied()
            {
                return Err(format!(
                    "{}: contents, length, or extrema differ",
                    context()
                ));
            }
        }
    }
    Ok(actual.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_generator_and_workloads() {
        let mut rng = Generator(0);
        assert_eq!(rng.next(), 0xe220a8397b1dcdaf);
        assert_eq!(rng.next(), 0x6e789e6aa1b965f4);
        assert_eq!(generate(256, 100, 42), generate(256, 100, 42));
        let ops = generate(256, 10_000, 42).unwrap();
        for candidate in [
            Operation::Insert(0),
            Operation::Remove(0),
            Operation::Contains(0),
            Operation::Predecessor(0),
            Operation::Successor(0),
            Operation::Minimum,
            Operation::Maximum,
        ] {
            assert!(
                ops.iter()
                    .any(|op| std::mem::discriminant(op) == std::mem::discriminant(&candidate))
            );
        }
    }
}
