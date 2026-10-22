use cgx2_cli::run_source_with_report;
use cgx2_memory_conformance::v3::{self, Fault, MODEL_VERSION_V3};
use cgx2_memory_conformance::v4::{self, Coverage, MODEL_VERSION_V4};
use cgx2_memory_conformance::v5::{self, MODEL_VERSION_V5};
use cgx2_memory_conformance::{generate, generate_v2, MODEL_VERSION, MODEL_VERSION_V2};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut raw_seed = None;
    let mut corpus_count = None;
    let mut version = MODEL_VERSION;
    let mut repeats = 2_u32;
    let mut reduce = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--seed" if raw_seed.is_none() => raw_seed = args.next(),
            "--corpus-count" if corpus_count.is_none() => corpus_count = args.next(),
            "--repeats" => {
                repeats = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0)
            }
            "--reduce" => reduce = true,
            "--model-version" => {
                version = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
            }
            _ => version = 0,
        }
    }
    if raw_seed.is_some() == corpus_count.is_some() {
        eprintln!("usage: cgx2-memory-conformance (--seed N | --corpus-count N) [--model-version 1|2|3|4|5] [--repeats N] [--reduce]");
        std::process::exit(2);
    }
    if version != MODEL_VERSION
        && version != MODEL_VERSION_V2
        && version != MODEL_VERSION_V3
        && version != MODEL_VERSION_V4
        && version != MODEL_VERSION_V5
    {
        eprintln!("unsupported model version: {version}; expected 1, 2, 3, 4, or 5");
        std::process::exit(2);
    }
    if version == MODEL_VERSION_V5 {
        if repeats == 0 || (reduce && raw_seed.is_none()) {
            eprintln!("v5 requires --repeats >= 1 and --reduce requires --seed");
            std::process::exit(2);
        }
        if let Some(raw_count) = corpus_count {
            let count = raw_count.parse::<u64>().unwrap_or_else(|_| {
                eprintln!("invalid unsigned corpus count: {raw_count}");
                std::process::exit(2);
            });
            let mut first = None;
            for _ in 0..repeats {
                let mut summary = v5::Coverage::default();
                for seed in 0..count {
                    summary.merge(&v5::run(seed).unwrap_or_else(|error| {
                        eprintln!("{error}");
                        std::process::exit(1);
                    }));
                }
                if first.as_ref().is_some_and(|baseline| baseline != &summary) {
                    eprintln!("v5 repeated-run coverage disagreement");
                    std::process::exit(1);
                }
                first = Some(summary);
            }
            println!(
                "{}",
                first
                    .expect("positive repeats")
                    .to_json(repeats, 0, Some(count))
            );
            return;
        }
        let raw_seed = raw_seed.expect("one invocation mode validated");
        let seed = raw_seed.parse::<u64>().unwrap_or_else(|_| {
            eprintln!("invalid unsigned seed: {raw_seed}");
            std::process::exit(2);
        });
        let trace = v5::generate(seed);
        if reduce {
            let reduced = v5::shrink(&trace, false);
            println!("{}", v5::format_trace(&reduced));
            return;
        }
        let mut first = None;
        for _ in 0..repeats {
            let summary = v5::run(seed).unwrap_or_else(|error| {
                eprintln!("{error}");
                std::process::exit(1);
            });
            if first.as_ref().is_some_and(|baseline| baseline != &summary) {
                eprintln!("v5 repeated-run coverage disagreement");
                std::process::exit(1);
            }
            first = Some(summary);
        }
        println!(
            "{}",
            first
                .expect("positive repeats")
                .to_json(repeats, seed, seed.checked_add(1))
        );
        return;
    }
    if let Some(raw_count) = corpus_count {
        if version != MODEL_VERSION_V4 {
            eprintln!("--corpus-count requires --model-version 4");
            std::process::exit(2);
        }
        let count = raw_count.parse::<u64>().unwrap_or_else(|_| {
            eprintln!("invalid unsigned corpus count: {raw_count}");
            std::process::exit(2);
        });
        let mut summary = Coverage::default();
        for seed in 0..count {
            summary.merge(&v4::run(seed, Fault::None).unwrap_or_else(|error| {
                eprintln!("{error}");
                std::process::exit(1);
            }));
        }
        println!("{}", summary.to_json());
        return;
    }
    let raw_seed = raw_seed.expect("one invocation mode validated");
    let seed = raw_seed.parse::<u64>().unwrap_or_else(|_| {
        eprintln!("invalid unsigned seed: {raw_seed}");
        std::process::exit(2);
    });
    if version == MODEL_VERSION_V4 {
        let trace = v4::generate(seed);
        println!(
            "model=4 seed={seed} steps={}\n{}",
            trace.len(),
            v3::format_trace(&trace)
        );
        match v4::run_trace(&trace, Fault::None) {
            Ok(coverage) => println!("result=pass\ncoverage={}", coverage.to_json()),
            Err(reason) => {
                eprintln!(
                    "mismatch: {reason}\noriginal trace:\n{}\nminimized trace:\n{}",
                    v3::format_trace(&trace),
                    v3::format_trace(&v4::shrink(&trace, Fault::None))
                );
                std::process::exit(1);
            }
        }
        return;
    }
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
