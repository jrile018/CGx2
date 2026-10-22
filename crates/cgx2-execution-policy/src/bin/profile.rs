//! Save a local timing profile or run the reviewed map with validated Auto.

use cgx2_execution_policy::profile::{
    build_profile_for_size, current_machine, load_profile, sample_hash,
};
use cgx2_execution_policy::{Capabilities, Policy, execute_map};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

fn args() -> Result<(String, BTreeMap<String, String>), String> {
    let mut iter = std::env::args().skip(1);
    let command = iter.next().ok_or("missing command: build or run")?;
    let mut options = BTreeMap::new();
    while let Some(flag) = iter.next() {
        if !flag.starts_with("--") {
            return Err(format!("invalid option {flag}"));
        }
        let value = iter
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        if options.insert(flag, value).is_some() {
            return Err("duplicate option".into());
        }
    }
    Ok((command, options))
}

fn required<'a>(options: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    options
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing {key}"))
}

fn number(options: &BTreeMap<String, String>, key: &str) -> Result<usize, String> {
    required(options, key)?
        .parse()
        .map_err(|_| format!("invalid {key}"))
}

fn run() -> Result<(), String> {
    let (command, options) = args()?;
    let machine = current_machine().map_err(|e| format!("machine: {e:?}"))?;
    let workers = number(&options, "--workers")?;
    match command.as_str() {
        "build" => {
            let allowed = ["--raw", "--out", "--workers", "--size", "--ttl-hours"];
            if options.keys().any(|key| !allowed.contains(&key.as_str())) {
                return Err("unknown build option".into());
            }
            let raw = std::fs::read_to_string(required(&options, "--raw")?)
                .map_err(|e| format!("raw samples: {e}"))?;
            let size = number(&options, "--size")?;
            let hours = options
                .get("--ttl-hours")
                .map_or(Ok(24_u64), |v| v.parse().map_err(|_| "invalid --ttl-hours"))?;
            let ttl = Duration::from_secs(hours.checked_mul(3600).ok_or("TTL overflow")?);
            let profile =
                build_profile_for_size(&raw, &machine, workers, size, SystemTime::now(), ttl)
                    .map_err(|e| format!("profile rejected: {e:?}"))?;
            std::fs::write(required(&options, "--out")?, profile.to_text())
                .map_err(|e| format!("write profile: {e}"))?;
            println!(
                "saved workload={} size={} workers={} calibration_samples={} heldout_samples={} heldout_max_regret_ppm={} raw_hash={}",
                profile.workload_id,
                size,
                workers,
                profile.calibration_samples,
                profile.heldout_samples,
                profile.heldout_max_regret_ppm,
                profile.raw_sample_hash
            );
        }
        "run" => {
            let allowed = ["--raw", "--profile", "--workers", "--sizes"];
            if options.keys().any(|key| !allowed.contains(&key.as_str())) {
                return Err("unknown run option".into());
            }
            let measurements = if options.contains_key("--profile") || options.contains_key("--raw")
            {
                let loaded = (|| {
                    let text = std::fs::read_to_string(required(&options, "--profile")?)
                        .map_err(|e| format!("profile: {e}"))?;
                    let raw = std::fs::read_to_string(required(&options, "--raw")?)
                        .map_err(|e| format!("raw samples: {e}"))?;
                    load_profile(&text, &raw, &machine, workers, SystemTime::now())
                        .map_err(|e| format!("{e:?}"))
                })();
                match loaded {
                    Ok(measurements) => Some(measurements),
                    Err(error) => {
                        eprintln!("profile rejected ({error}); Auto uses sequential fallback");
                        None
                    }
                }
            } else {
                None
            };
            let caps = Capabilities {
                parallel_workers: workers,
                measurements,
                ..Capabilities::default()
            };
            for size in required(&options, "--sizes")?.split(',') {
                let size: usize = size.parse().map_err(|_| "invalid --sizes")?;
                if size > 100_000_000 {
                    return Err("size too large".into());
                }
                let input: Vec<i64> = (0..size).map(|i| (i % 97) as i64 - 48).collect();
                let execution = execute_map(&input, 3, 7, Policy::Auto, &caps)
                    .map_err(|e| format!("execution: {e:?}"))?;
                let bytes: Vec<u8> = execution
                    .values
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                println!(
                    "size={size} backend={:?} reason={:?} cost={:?} result_hash={}",
                    execution.decision.selected,
                    execution.decision.reason,
                    execution.decision.cost,
                    sample_hash(&bytes)
                );
            }
        }
        _ => return Err("expected build or run command".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!(
            "{error}\nusage: profile build --raw SAMPLES.csv --out PROFILE.txt --workers N --size N [--ttl-hours 24]\n       profile run [--raw SAMPLES.csv --profile PROFILE.txt] --workers N --sizes N,N"
        );
        std::process::exit(2);
    }
}
