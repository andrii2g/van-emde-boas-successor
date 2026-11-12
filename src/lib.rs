//! An eager, bounded-universe van Emde Boas set.
//!
//! Keys are in `0..universe`. Predecessor and successor are strict.
//! Construction and storage are O(U); set operations are O(log log U),
//! except cached minimum, maximum, and length, which are O(1).

pub mod benchmark;
mod metrics;
mod veb;
pub mod workload;

pub use metrics::{Metrics, OPERATION_NAMES, Storage};

pub use veb::{KeyError, UniverseError, VebSet};
