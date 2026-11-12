use std::{collections::BTreeMap, error::Error, fs, path::Path, process::ExitCode};
use veb_successor::{
    benchmark::{self, Config, Workload},
    workload,
};

const HELP: &str = "van-emde-boas-successor

Usage:
  veb verify [--universe 16|256|65536|all] [--operations 10000] [--seed 42]
  veb bench  [--universe 16|256|65536|all] [--operations 10000] [--seed 42]
             [--occupancy 0..100|all] [--repeats 7] [--workload queries|mixed]
             [--output results.csv]

Defaults: universe=all, occupancy=all (1%, 50%, 90%), workload=queries.
Use cargo run --release -- ... for timings. Bench emits CSV; empty BTreeSet
storage/counter fields mean unmeasured. Mixed occupancy is initial occupancy.
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print!("{HELP}");
        return Ok(());
    };
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        print!("{HELP}");
        return Ok(());
    }
    let allowed: &[&str] = match command.as_str() {
        "verify" => &["--universe", "--operations", "--seed"],
        "bench" => &[
            "--universe",
            "--operations",
            "--seed",
            "--occupancy",
            "--repeats",
            "--workload",
            "--output",
        ],
        _ => return Err(format!("unknown command {command:?}; use --help").into()),
    };
    let mut options = BTreeMap::new();
    while let Some(flag) = args.next() {
        if matches!(flag.as_str(), "--help" | "-h") {
            print!("{HELP}");
            return Ok(());
        }
        if !allowed.contains(&flag.as_str()) {
            return Err(format!("unknown option {flag:?} for {command}").into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        if options.insert(flag.clone(), value).is_some() {
            return Err(format!("duplicate option {flag}").into());
        }
    }
    let get =
        |flag: &str, default: &str| options.get(flag).cloned().unwrap_or_else(|| default.into());
    let universes = match get("--universe", "all").as_str() {
        "all" => vec![16, 256, 65_536],
        value => vec![value.parse::<u32>()?],
    };
    let count: usize = get("--operations", "10000").parse()?;
    if count == 0 {
        return Err("operations must be positive".into());
    }
    let seed = get("--seed", "42").parse()?;
    if command == "verify" {
        for universe in universes {
            let len = workload::verify(universe, count, seed)?;
            println!("PASS universe={universe} operations={count} seed={seed} final_len={len}");
        }
    } else {
        let occupancies = match get("--occupancy", "all").as_str() {
            "all" => vec![1, 50, 90],
            value => vec![value.parse::<u32>()?],
        };
        let repeats = get("--repeats", "7").parse()?;
        let workload = match get("--workload", "queries").as_str() {
            "queries" => Workload::Queries,
            "mixed" => Workload::Mixed,
            _ => return Err("workload must be queries or mixed".into()),
        };
        let mut csv = benchmark::csv_header();
        for universe in universes {
            for &occupancy in &occupancies {
                csv.push_str(
                    &benchmark::run(Config {
                        universe,
                        operations: count,
                        seed,
                        occupancy,
                        repeats,
                        workload,
                    })?
                    .csv_rows(),
                );
            }
        }
        if let Some(path) = options.get("--output") {
            write_output(path, &csv)?;
            println!("Wrote {path}");
        } else {
            print!("{csv}");
        }
    }
    Ok(())
}

fn write_output(path: &str, contents: &str) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}
