use cgx2_cli::run_source_with_report;
use cgx2_memory_conformance::{generate, MODEL_VERSION};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(flag), Some(raw_seed), None) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: cgx2-memory-conformance --seed N");
        std::process::exit(2);
    };
    if flag != "--seed" {
        eprintln!("usage: cgx2-memory-conformance --seed N");
        std::process::exit(2);
    }
    let seed = raw_seed.parse::<u64>().unwrap_or_else(|_| {
        eprintln!("invalid unsigned seed: {raw_seed}");
        std::process::exit(2);
    });
    let case = generate(seed);
    println!(
        "model={MODEL_VERSION} seed={seed}\nsource:\n{}expected={:#?}",
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
