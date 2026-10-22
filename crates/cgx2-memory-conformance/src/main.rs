use cgx2_cli::run_source_with_report;
use cgx2_memory_conformance::v3::{self, Fault, MODEL_VERSION_V3};
use cgx2_memory_conformance::{generate, generate_v2, MODEL_VERSION, MODEL_VERSION_V2};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut raw_seed = None;
    let mut version = MODEL_VERSION;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--seed" if raw_seed.is_none() => raw_seed = args.next(),
            "--model-version" => {
                version = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
            }
            _ => version = 0,
        }
    }
    let Some(raw_seed) = raw_seed else {
        eprintln!("usage: cgx2-memory-conformance --seed N [--model-version 1|2|3]");
        std::process::exit(2);
    };
    if version != MODEL_VERSION && version != MODEL_VERSION_V2 && version != MODEL_VERSION_V3 {
        eprintln!("unsupported model version: {version}; expected 1, 2, or 3");
        std::process::exit(2);
    }
    let seed = raw_seed.parse::<u64>().unwrap_or_else(|_| {
        eprintln!("invalid unsigned seed: {raw_seed}");
        std::process::exit(2);
    });
    if version == MODEL_VERSION_V3 {
        let trace = v3::generate(seed);
        println!(
            "model=3 seed={seed} steps={}\n{}",
            trace.len(),
            v3::format_trace(&trace)
        );
        if let Err(reason) = v3::run_trace(&trace, Fault::None) {
            eprintln!(
                "mismatch: {reason}\noriginal trace:\n{}\nminimized trace:\n{}",
                v3::format_trace(&trace),
                v3::format_trace(&v3::shrink(&trace, Fault::None))
            );
            std::process::exit(1);
        }
        println!("result=pass");
        return;
    }
    let case = if version == MODEL_VERSION {
        generate(seed)
    } else {
        generate_v2(seed)
    };
    println!(
        "model={version} seed={seed}\nsource:\n{}expected={:#?}",
        case.source, case.expected
    );
    match run_source_with_report(&case.source, &[]) {
        Ok((_, Some(report))) => {
            println!("actual counters:\n{}", report.to_text());
            if let Err(reason) = case.expected.compare(&report) {
                eprintln!("mismatch: {reason}\nreport={report:#?}");
                std::process::exit(1);
            }
        }
        outcome => {
            eprintln!("unexpected outcome: {outcome:#?}");
            std::process::exit(1);
        }
    }
}
