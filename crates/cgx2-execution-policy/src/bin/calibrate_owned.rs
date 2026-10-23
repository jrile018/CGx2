//! Quiet-host handoff for the versioned owned API. Never run by dispatch.
use cgx2_execution_policy::owned_v1::{
    self as owned, Budgets, Context, Input, Operation, ValidatedProfile,
};
use cgx2_execution_policy::profile::{MachineFingerprint, current_machine};
use cgx2_execution_policy::{MAX_WORKERS, Policy};
use std::hint::black_box;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime};

fn input(operation: Operation, elements: usize) -> Input {
    match operation {
        Operation::Map => Input::Map {
            values: vec![2; elements],
            multiplier: 3,
            addend: 7,
        },
        Operation::ZipMap => Input::ZipMap {
            left: vec![2; elements],
            right: vec![3; elements],
            addend: 7,
        },
    }
}

fn samples(operation: Operation, elements: usize, context: Context<'_>, count: usize) -> String {
    let machine = context.machine;
    let mut csv = "schema,phase,kind,elements,workers,sample,elapsed_ns,os,arch,cpu,available_threads,rustc\n".to_string();
    for (phase, repetitions) in [("calibration", count), ("holdout", count.max(5))] {
        for sample in 0..repetitions {
            let mut candidates = [
                ("sequential", Policy::SequentialCpu),
                ("parallel", Policy::ParallelCpu),
            ];
            if sample % 2 != 0 {
                candidates.reverse();
            }
            for (kind, policy) in candidates {
                // Buffer creation is charged to the source adapter separately.
                let data = input(operation, elements);
                let cancellation = AtomicBool::new(false);
                let start = Instant::now();
                let outcome = black_box(owned::execute(
                    black_box(data),
                    policy,
                    context,
                    &cancellation,
                    Budgets::default(),
                ));
                let elapsed = start.elapsed().as_nanos();
                assert_eq!(outcome.result.unwrap(), vec![13; elements]);
                csv.push_str(&format!(
                    "{},{phase},{kind},{elements},{},{sample},{elapsed},{},{},{},{},{}\n",
                    operation.identity().0,
                    context.parallel_workers,
                    machine.os,
                    machine.arch,
                    machine.cpu,
                    machine.available_threads,
                    machine.rustc
                ));
            }
        }
    }
    csv
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 6 || args[0] != "--quiet-host-confirmed" {
        return Err("usage: calibrate_owned --quiet-host-confirmed map|zip ELEMENTS WORKERS RAW.csv PROFILE.txt; run only on a quiet calibrated host".into());
    }
    let operation = match args[1].as_str() {
        "map" => Operation::Map,
        "zip" => Operation::ZipMap,
        _ => return Err("operation must be map or zip".into()),
    };
    let elements: usize = args[2].parse().map_err(|_| "invalid elements")?;
    let workers: usize = args[3].parse().map_err(|_| "invalid workers")?;
    if !(2048..=100_000_000).contains(&elements)
        || !(2..=MAX_WORKERS).contains(&workers)
        || workers > elements / 1024
    {
        return Err(
            "require 2048..=100000000 elements and 2..=32 workers, at least 1024 elements each"
                .into(),
        );
    }
    if args[4] == args[5] {
        return Err("raw and profile paths must differ".into());
    }
    if std::path::Path::new(&args[4]).exists() || std::path::Path::new(&args[5]).exists() {
        return Err("output paths must be fresh; preserve prior evidence".into());
    }
    let machine: MachineFingerprint = current_machine().map_err(|e| format!("{e:?}"))?;
    let context = Context {
        parallel_workers: workers,
        machine: &machine,
        profile: None,
    };
    // Verify both backends, then warm both before collecting bootstrap evidence.
    for _ in 0..2 {
        for policy in [Policy::SequentialCpu, Policy::ParallelCpu] {
            assert_eq!(
                owned::execute(
                    input(operation, elements),
                    policy,
                    context,
                    &AtomicBool::new(false),
                    Budgets::default()
                )
                .result
                .unwrap(),
                vec![13; elements]
            );
        }
    }
    let bootstrap = samples(operation, elements, context, 3);
    let now = SystemTime::now();
    let provisional = owned::build_profile(
        operation,
        &bootstrap,
        &machine,
        workers,
        elements,
        now,
        Duration::from_secs(86400),
    )
    .map_err(|e| format!("bootstrap rejected: {e:?}; retry on a quieter host"))?;
    let validated = ValidatedProfile::load(
        operation,
        &provisional.to_text(),
        &bootstrap,
        &machine,
        workers,
        now,
    )
    .map_err(|e| format!("{e:?}"))?;
    // The recorded runs include accepted-profile validation and report costs.
    let csv = samples(
        operation,
        elements,
        Context {
            profile: Some(&validated),
            ..context
        },
        7,
    );
    save_new(&args[4], &csv)?;
    let profile = owned::build_profile(
        operation,
        &csv,
        &machine,
        workers,
        elements,
        SystemTime::now(),
        Duration::from_secs(86400),
    )
    .map_err(|e| format!("recorded evidence rejected: {e:?}; raw saved, no profile published"))?;
    save_new(&args[5], &profile.to_text())?;
    println!("saved exact-size owned profile; this is calibration, not a release performance gate");
    Ok(())
}
fn save_new(path: &str, content: &str) -> Result<(), String> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(content.as_bytes()))
        .map_err(|e| e.to_string())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
