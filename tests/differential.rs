use veb_successor::{VebSet, workload::verify};

#[test]
fn ten_thousand_operations_per_universe() {
    for universe in [16, 256, 65_536] {
        verify(universe, 10_000, 42).unwrap();
    }
}

#[test]
fn fill_and_drain_across_every_cluster() {
    for universe in [16, 256, 65_536] {
        let mut set = VebSet::new(universe).unwrap();
        for key in (0..universe).rev() {
            assert!(set.insert(key).unwrap());
        }
        set.validate().unwrap();
        for key in 0..universe {
            assert_eq!(set.minimum(), Some(key));
            assert_eq!(
                set.successor(key).unwrap(),
                (key + 1 < universe).then_some(key + 1)
            );
            assert!(set.remove(key).unwrap());
        }
        assert!(set.is_empty());
        set.validate().unwrap();
        for key in 0..universe {
            set.insert(key).unwrap();
        }
        for key in (0..universe).rev() {
            assert_eq!(set.maximum(), Some(key));
            assert_eq!(set.predecessor(key).unwrap(), key.checked_sub(1));
            assert!(set.remove(key).unwrap());
        }
        set.validate().unwrap();
    }
}
