use std::process::Command;
use veb_successor::{Query, SvgOptions, VebSet};

#[test]
fn svg_is_deterministic_bounded_and_observational() {
    let mut set = VebSet::new(16).unwrap();
    for key in [2, 3, 7, 9, 14] {
        set.insert(key).unwrap();
    }
    let before = set.metrics();
    let options = SvgOptions {
        show_empty: true,
        query: Some(Query::Successor(7)),
        ..SvgOptions::default()
    };
    let svg = set.to_svg(options).unwrap();
    assert_eq!(svg, set.to_svg(options).unwrap());
    assert_eq!(svg.matches("<g id=").count(), 21);
    assert!(svg.contains("Successor(7) = 9"));
    assert!(svg.contains("stroke-dasharray"));
    assert!(svg.contains("summary IDs [0..4)"));
    assert!(svg.contains("root keys [8..12)"));
    assert_eq!(set.metrics(), before);
    let collapsed = set
        .to_svg(SvgOptions {
            max_depth: 0,
            ..options
        })
        .unwrap();
    assert_eq!(collapsed.matches("<g id=").count(), 1);
    assert!(collapsed.contains("collapsed"));
    let limited = set
        .to_svg(SvgOptions {
            max_nodes: 3,
            ..options
        })
        .unwrap();
    assert_eq!(limited.matches("<g id=").count(), 3);
    assert!(limited.contains("collapsed"));
    assert!(
        set.to_svg(SvgOptions {
            max_nodes: 0,
            ..options
        })
        .is_err()
    );
    assert!(
        set.to_svg(SvgOptions {
            query: Some(Query::Predecessor(16)),
            ..options
        })
        .is_err()
    );
}

#[test]
fn empty_and_large_diagrams_respect_limits() {
    let mut set = VebSet::new(65_536).unwrap();
    assert_eq!(
        set.to_svg(SvgOptions::default())
            .unwrap()
            .matches("<g id=")
            .count(),
        1
    );
    for key in 0..65_536 {
        set.insert(key).unwrap();
    }
    let svg = set
        .to_svg(SvgOptions {
            max_nodes: 12,
            ..SvgOptions::default()
        })
        .unwrap();
    assert_eq!(svg.matches("<g id=").count(), 12);
}

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_veb"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_reports_errors_and_runs_experiments() {
    assert!(cli(&["--help"]).status.success());
    for args in [
        vec!["unknown"],
        vec!["verify", "--universe", "17"],
        vec!["verify", "--operations", "0"],
        vec!["verify", "--seed"],
        vec!["verify", "--seed", "1", "--seed", "2"],
        vec!["bench", "--repeats", "0"],
        vec!["bench", "--workload", "bad"],
        vec!["render", "--keys", "16"],
        vec!["render", "--max-nodes", "0"],
        vec!["render", "--successor", "16"],
        vec!["render", "--successor", "1", "--predecessor", "2"],
        vec!["verify", "--output", "forbidden.csv"],
    ] {
        let result = cli(&args);
        assert!(!result.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("error:"));
    }
    let verified = cli(&["verify", "--universe", "16", "--operations", "100"]);
    assert!(verified.status.success());
    assert!(String::from_utf8_lossy(&verified.stdout).contains("PASS"));
    let bench = cli(&[
        "bench",
        "--universe",
        "16",
        "--operations",
        "100",
        "--repeats",
        "1",
        "--occupancy",
        "50",
    ]);
    assert!(bench.status.success());
    assert_eq!(String::from_utf8_lossy(&bench.stdout).lines().count(), 3);
    let path = std::env::temp_dir().join(format!("veb-svg-cli-{}.svg", std::process::id()));
    let result = cli(&[
        "render",
        "--show-empty",
        "--successor",
        "7",
        "--output",
        path.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let svg = std::fs::read_to_string(&path).unwrap();
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("Successor(7) = 9"));
    std::fs::remove_file(path).unwrap();
}
