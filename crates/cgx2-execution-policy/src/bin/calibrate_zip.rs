//! Repeatable local calibration of the checked two-input CPU primitive.
use cgx2_execution_policy::profile::{
    ZIP_SCHEMA_VERSION, build_zip_profile_for_size, current_machine, load_zip_profile,
};
use cgx2_execution_policy::{Capabilities, Policy, execute_zip_map};
use std::hint::black_box;
use std::time::{Duration, Instant, SystemTime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: calibrate_zip SIZE WORKERS RAW_CSV PROFILE_TXT".into());
    }
    let size: usize = args[0].parse()?;
    let workers: usize = args[1].parse()?;
    let machine = current_machine().map_err(|e| format!("machine: {e:?}"))?;
    let left: Vec<i64> = (0..size).map(|i| (i % 97) as i64 - 48).collect();
    let right: Vec<i64> = (0..size).map(|i| (i % 17) as i64 - 8).collect();
    let caps = Capabilities {
        parallel_workers: workers,
        ..Capabilities::default()
    };
    let run = |policy| execute_zip_map(&left, &right, 7, policy, &caps);
    if run(Policy::SequentialCpu)
        .map_err(|e| format!("sequential: {e:?}"))?
        .values
        != run(Policy::ParallelCpu)
            .map_err(|e| format!("parallel: {e:?}"))?
            .values
    {
        return Err("backend result mismatch".into());
    }
    for _ in 0..2 {
        black_box(run(Policy::SequentialCpu).unwrap());
        black_box(run(Policy::ParallelCpu).unwrap());
    }
    let mut csv = String::from(
        "schema,phase,kind,elements,workers,sample,elapsed_ns,os,arch,cpu,available_threads,rustc\n",
    );
    for (phase, count) in [("calibration", 7), ("holdout", 5)] {
        for sample in 0..count {
            // Reverse each second pair to reduce a fixed ordering effect.
            let order = if sample % 2 == 0 {
                [
                    ("sequential", Policy::SequentialCpu),
                    ("parallel", Policy::ParallelCpu),
                ]
            } else {
                [
                    ("parallel", Policy::ParallelCpu),
                    ("sequential", Policy::SequentialCpu),
                ]
            };
            for (kind, policy) in order {
                let start = Instant::now();
                black_box(run(policy).map_err(|e| format!("execution: {e:?}"))?);
                let elapsed = start.elapsed().as_nanos();
                csv.push_str(&format!(
                    "{ZIP_SCHEMA_VERSION},{phase},{kind},{size},{workers},{sample},{elapsed},{},{},{},{},{}\n",
                    machine.os, machine.arch, machine.cpu, machine.available_threads, machine.rustc
                ));
            }
        }
    }
    std::fs::write(&args[2], &csv)?;
    let now = SystemTime::now();
    match build_zip_profile_for_size(
        &csv,
        &machine,
        workers,
        size,
        now,
        Duration::from_secs(86_400),
    ) {
        Ok(profile) => {
            std::fs::write(&args[3], profile.to_text())?;
            let measurements = load_zip_profile(&profile.to_text(), &csv, &machine, workers, now)
                .map_err(|e| format!("load: {e:?}"))?;
            let caps = Capabilities {
                parallel_workers: workers,
                measurements: Some(measurements),
                ..Capabilities::default()
            };
            let execution = execute_zip_map(&left, &right, 7, Policy::Auto, &caps)
                .map_err(|e| format!("auto: {e:?}"))?;
            println!(
                "size={size} workers={workers} seq_ns=[{},{}] par_ns=[{},{}] heldout_max_regret_ppm={} auto={:?} reason={:?}",
                profile.sequential_cost.launch_setup.lower_ns,
                profile.sequential_cost.launch_setup.upper_ns,
                profile.parallel_cost.launch_setup.lower_ns,
                profile.parallel_cost.launch_setup.upper_ns,
                profile.heldout_max_regret_ppm,
                execution.decision.selected,
                execution.decision.reason
            );
        }
        Err(error) => {
            println!("raw samples saved; profile rejected: {error:?}; Auto stays sequential");
        }
    }
    Ok(())
}
