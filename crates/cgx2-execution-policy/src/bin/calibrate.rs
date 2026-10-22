//! Opt-in raw timing recorder. It never writes a policy profile automatically.

use cgx2_execution_policy::profile::current_machine;
use cgx2_execution_policy::{Capabilities, MAX_WORKERS, Policy, execute_map};
use std::collections::BTreeSet;
use std::hint::black_box;
use std::time::Instant;

struct Config {
    sizes: Vec<usize>,
    workers: Vec<usize>,
    samples: usize,
    holdout_samples: usize,
    warmups: usize,
}

fn parse_list(value: &str) -> Result<Vec<usize>, String> {
    let values = value
        .split(',')
        .map(|part| {
            part.parse::<usize>()
                .map_err(|_| format!("invalid number: {part}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.is_empty() || values.contains(&0) {
        return Err("lists must contain positive integers".into());
    }
    Ok(values)
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config {
        sizes: vec![4_096, 65_536],
        workers: vec![2, 4],
        samples: 7,
        holdout_samples: 5,
        warmups: 2,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--sizes" => config.sizes = parse_list(&value)?,
            "--workers" => config.workers = parse_list(&value)?,
            "--samples" => config.samples = value.parse().map_err(|_| "invalid samples")?,
            "--holdout-samples" => {
                config.holdout_samples = value.parse().map_err(|_| "invalid holdout samples")?
            }
            "--warmups" => config.warmups = value.parse().map_err(|_| "invalid warmups")?,
            _ => return Err(format!("unknown option: {flag}")),
        }
    }
    if !(3..=1_000).contains(&config.samples)
        || !(3..=1_000).contains(&config.holdout_samples)
        || config.sizes.len() > 32
        || config.workers.len() > 16
        || config.sizes.iter().collect::<BTreeSet<_>>().len() != config.sizes.len()
        || config.workers.iter().collect::<BTreeSet<_>>().len() != config.workers.len()
        || config
            .sizes
            .iter()
            .any(|&size| !(2_048..=100_000_000).contains(&size))
        || config
            .workers
            .iter()
            .any(|&w| !(2..=MAX_WORKERS).contains(&w))
        || config
            .sizes
            .iter()
            .any(|&size| config.workers.iter().any(|&workers| size / 1_024 < workers))
    {
        return Err(format!(
            "sizes must be unique in 2048..=100000000 and allow 1024 elements per worker; calibration and holdout samples must each be 3..=1000; workers must be unique in 2..={MAX_WORKERS}"
        ));
    }
    Ok(config)
}

fn time_ns(f: impl FnOnce()) -> u128 {
    let start = Instant::now();
    f();
    start.elapsed().as_nanos()
}

fn run(config: Config) {
    let machine = current_machine().expect("machine fingerprint required for calibration");
    println!(
        "schema,phase,kind,elements,workers,sample,elapsed_ns,os,arch,cpu,available_threads,rustc"
    );
    for &size in &config.sizes {
        let input: Vec<i64> = (0..size).map(|i| (i % 97) as i64 - 48).collect();
        for &workers in &config.workers {
            let caps = Capabilities {
                parallel_workers: workers,
                ..Capabilities::default()
            };
            assert_eq!(
                execute_map(&input, 3, 7, Policy::SequentialCpu, &caps)
                    .unwrap()
                    .values,
                execute_map(&input, 3, 7, Policy::ParallelCpu, &caps)
                    .unwrap()
                    .values,
                "calibration input must agree before timing"
            );
            for _ in 0..config.warmups {
                black_box(
                    execute_map(black_box(&input), 3, 7, Policy::SequentialCpu, &caps).unwrap(),
                );
                black_box(
                    execute_map(black_box(&input), 3, 7, Policy::ParallelCpu, &caps).unwrap(),
                );
            }
            for (phase, samples) in [
                ("calibration", config.samples),
                ("holdout", config.holdout_samples),
            ] {
                for sample in 0..samples {
                    for (kind, policy) in [
                        ("sequential", Policy::SequentialCpu),
                        ("parallel", Policy::ParallelCpu),
                    ] {
                        let elapsed = time_ns(|| {
                            black_box(execute_map(black_box(&input), 3, 7, policy, &caps).unwrap());
                        });
                        println!(
                            "1,{phase},{kind},{size},{workers},{sample},{elapsed},{},{},{},{},{}",
                            machine.os,
                            machine.arch,
                            machine.cpu,
                            machine.available_threads,
                            machine.rustc
                        );
                    }
                }
            }
        }
    }
}

fn main() {
    match parse_args() {
        Ok(config) => run(config),
        Err(error) => {
            eprintln!(
                "{error}\nusage: calibrate [--sizes 4096,65536] [--workers 2,4] [--samples 7] [--holdout-samples 5] [--warmups 2]"
            );
            std::process::exit(2);
        }
    }
}
