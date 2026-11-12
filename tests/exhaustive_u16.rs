use std::collections::BTreeSet;
use veb_successor::{VebSet, workload::Operation};

#[test]
fn every_subset_and_every_possible_single_key_update() {
    // Gray code reaches every subset using both insertion and removal transitions.
    let mut set = VebSet::new(16).unwrap();
    let mut oracle = BTreeSet::new();
    let mut previous = 0u32;
    for index in 0u32..65_536 {
        let mask = index ^ (index >> 1);
        if index != 0 {
            let key = (mask ^ previous).trailing_zeros();
            let op = if mask & (1 << key) != 0 {
                Operation::Insert(key)
            } else {
                Operation::Remove(key)
            };
            assert_eq!(op.apply_veb(&mut set).unwrap(), op.apply_btree(&mut oracle));
        }
        previous = mask;
        set.validate().unwrap();
        assert_eq!(
            set.to_vec(),
            oracle.iter().copied().collect::<Vec<_>>(),
            "subset {mask:#06x}"
        );
        assert_eq!(set.len(), oracle.len());
        for key in 0..16 {
            for op in [
                Operation::Contains(key),
                Operation::Predecessor(key),
                Operation::Successor(key),
                Operation::Minimum,
                Operation::Maximum,
            ] {
                assert_eq!(
                    op.apply_veb(&mut set).unwrap(),
                    op.apply_btree(&mut oracle),
                    "subset {mask:#06x}, {op:?}"
                );
            }
            // Exercise both possible updates from every subset, then restore it.
            let present = oracle.contains(&key);
            assert_eq!(set.insert(key).unwrap(), !present);
            set.validate().unwrap();
            assert_eq!(
                set.to_vec(),
                (0..16)
                    .filter(|&k| k == key || mask & (1 << k) != 0)
                    .collect::<Vec<_>>()
            );
            assert!(set.remove(key).unwrap());
            set.validate().unwrap();
            assert_eq!(
                set.to_vec(),
                (0..16)
                    .filter(|&k| k != key && mask & (1 << k) != 0)
                    .collect::<Vec<_>>()
            );
            assert!(!set.remove(key).unwrap());
            if present {
                set.insert(key).unwrap();
            }
        }
    }
}
