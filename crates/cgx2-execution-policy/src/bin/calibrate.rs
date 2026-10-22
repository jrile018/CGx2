//! Opt-in raw timing recorder. It never writes a policy profile automatically.

use cgx2_execution_policy::{MAX_WORKERS, map_i64_checked_parallel, map_i64_checked_sequential};
use std::hint::black_box;
use std::time::Instant;

struct Config {
    sizes: Vec<usize>,
    workers: Vec<usize>,
    samples: usize,
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
        sizes: vec![2_048, 4_096, 65_536],
        workers: vec![2, 4],
        samples: 7,
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
            "--warmups" => config.warmups = value.parse().map_err(|_| "invalid warmups")?,
            _ => return Err(format!("unknown option: {flag}")),
        }
    }
    if config.samples == 0
        || config
            .workers
            .iter()
            .any(|&w| !(2..=MAX_WORKERS).contains(&w))
    {
        return Err(format!(
            "samples must be positive; workers must be 2..={MAX_WORKERS}"
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
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("kind,elements,workers,sample,elapsed_ns,os,arch,available_threads,rustc");
    for &size in &config.sizes {
        let input: Vec<i64> = (0..size).map(|i| (i % 97) as i64 - 48).collect();
        for &workers in &config.workers {
            assert_eq!(
                map_i64_checked_sequential(&input, 3, 7),
                map_i64_checked_parallel(&input, 3, 7, workers),
                "calibration input must agree before timing"
            );
            for _ in 0..config.warmups {
                black_box(map_i64_checked_sequential(black_box(&input), 3, 7).unwrap());
                black_box(map_i64_checked_parallel(black_box(&input), 3, 7, workers).unwrap());
            }
            for sample in 0..config.samples {
                let scalar = time_ns(|| {
                    black_box(map_i64_checked_sequential(black_box(&input), 3, 7).unwrap());
                });
                let parallel = time_ns(|| {
                    black_box(map_i64_checked_parallel(black_box(&input), 3, 7, workers).unwrap());
                });
                let overhead = time_ns(|| {
                    std::thread::scope(|scope| {
                        let handles: Vec<_> = (0..workers).map(|_| scope.spawn(|| {})).collect();
                        for handle in handles {
                            handle.join().unwrap();
                        }
                    });
                });
                for (kind, elapsed) in [
                    ("scalar", scalar),
                    ("parallel", parallel),
                    ("scope_overhead", overhead),
                ] {
                    println!(
                        "{kind},{size},{workers},{sample},{elapsed},{},{},{threads},{rustc}",
                        std::env::consts::OS,
                        std::env::consts::ARCH
                    );
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
                "{error}\nusage: calibrate [--sizes 2048,4096,65536] [--workers 2,4] [--samples 7] [--warmups 2]"
            );
            std::process::exit(2);
        }
    }
}
