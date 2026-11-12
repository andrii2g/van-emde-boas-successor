use veb_successor::{
    Metrics, VebSet,
    benchmark::{self, Config, Workload},
    workload,
};

#[test]
fn instrumentation_is_optional_and_inspection_does_not_count() {
    for universe in [16, 256, 65_536] {
        let mut enabled = VebSet::new(universe).unwrap();
        let mut disabled = VebSet::new(universe).unwrap();
        disabled.set_metrics_enabled(false);
        for operation in workload::generate(universe, 1000, 17).unwrap() {
            assert_eq!(
                operation.apply_veb(&mut enabled),
                operation.apply_veb(&mut disabled)
            );
        }
        assert_eq!(enabled.to_vec(), disabled.to_vec());
        assert_eq!(disabled.metrics(), Metrics::default());
        let metrics = enabled.metrics();
        assert_eq!(metrics.operations.iter().sum::<u64>(), 1000);
        assert!(metrics.recursive_calls > 0);
        assert!(metrics.cluster_accesses > 0);
        assert!(metrics.summary_accesses > 0);
        assert!(metrics.max_depth <= universe.trailing_zeros().ilog2());
        enabled.validate().unwrap();
        enabled.storage();
        enabled.to_vec();
        assert_eq!(enabled.metrics(), metrics);
        enabled.reset_metrics();
        assert_eq!(enabled.metrics(), Metrics::default());
    }
}

#[test]
fn eager_storage_is_independent_of_occupancy() {
    for (universe, nodes) in [(16, 21), (256, 358), (65_536, 92_007)] {
        let mut set = VebSet::new(universe).unwrap();
        let before = set.storage();
        assert_eq!(before.allocated_nodes, nodes);
        assert_eq!(before.nonempty_nodes, 0);
        for key in 0..universe {
            set.insert(key).unwrap();
        }
        let after = set.storage();
        assert_eq!(before.owned_bytes, after.owned_bytes);
        assert_eq!(before.heap_allocations, after.heap_allocations);
        assert_eq!(after.allocated_nodes, nodes);
        assert!(after.nonempty_nodes > 0);
    }
}

#[test]
fn benchmark_preflight_replays_both_workloads_and_csv_has_matching_columns() {
    for workload in [Workload::Queries, Workload::Mixed] {
        for occupancy in [0, 1, 50, 90, 100] {
            let report = benchmark::run(Config {
                universe: 16,
                operations: 1000,
                seed: 42,
                occupancy,
                repeats: 2,
                workload,
            })
            .unwrap();
            assert_eq!(report.veb_batch_ns.len(), 2);
            assert_eq!(report.metrics.operations.iter().sum::<u64>(), 1000);
            if workload == Workload::Queries {
                assert_eq!(report.initial_len, report.final_len);
            }
            let columns = benchmark::csv_header().trim().split(',').count();
            for row in report.csv_rows().lines() {
                assert_eq!(row.split(',').count(), columns);
            }
        }
    }
    for config in [
        Config {
            universe: 16,
            operations: 0,
            seed: 42,
            occupancy: 50,
            repeats: 1,
            workload: Workload::Queries,
        },
        Config {
            universe: 16,
            operations: 100,
            seed: 42,
            occupancy: 101,
            repeats: 1,
            workload: Workload::Queries,
        },
    ] {
        assert!(benchmark::run(config).is_err());
    }
}
