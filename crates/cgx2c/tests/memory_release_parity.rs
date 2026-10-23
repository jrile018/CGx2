//! Bounded source/native memory unwind evidence. See the owned Wave 20 gate.
use cgx2_cli::{
    MemoryReport, Value, run_checked_source, run_checked_source_with_failure,
    run_checked_source_with_failure_at_reserve,
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-memory-release-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name)
}

fn field(json: &str, key: &str) -> u64 {
    let tail = json
        .split_once(&format!("\"{key}\":"))
        .unwrap_or_else(|| panic!("missing {key}"))
        .1;
    tail.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

fn totals(json: &str) -> Vec<u64> {
    let tail = json.rsplit_once("\"totals\":").unwrap().1;
    [
        "committed_bytes",
        "live_bytes",
        "peak_bytes",
        "allocation_failures",
        "collection_count",
        "total_collected_bytes",
    ]
    .iter()
    .map(|key| field(tail, key))
    .collect()
}

fn report_totals(report: &MemoryReport) -> Vec<u64> {
    totals(&report.to_json())
}

fn without_session_ids(json: &str) -> String {
    let mut result = String::new();
    let mut rest = json;
    while let Some((before, after)) = rest.split_once("\"session\":") {
        result.push_str(before);
        result.push_str("\"session\":_");
        rest = after.trim_start_matches(|ch: char| ch.is_ascii_digit());
    }
    result.push_str(rest);
    result
}

fn build_and_run(
    name: &str,
    release: bool,
    fault: Option<usize>,
    scratch: &Scratch,
) -> (std::process::Output, String) {
    let binary = scratch.path(&format!("{}-{}-{:?}", name, release, fault));
    let report = scratch.path(&format!("{}-{}-{:?}.json", name, release, fault));
    let mut build = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
    build.args([
        "build",
        source(name).to_str().unwrap(),
        "--out",
        binary.to_str().unwrap(),
    ]);
    if release {
        build.arg("--release");
    }
    if let Some(index) = fault {
        build.args(["--test-fail-reserve", &index.to_string()]);
    }
    let built = build.output().unwrap();
    assert!(
        built.status.success(),
        "{}: {}",
        name,
        String::from_utf8_lossy(&built.stderr)
    );
    let output = Command::new(&binary)
        .env("CGX2_MEMORY_REPORT", &report)
        .output()
        .unwrap();
    (output, fs::read_to_string(report).unwrap())
}

#[test]
fn normal_and_nested_no_gc_match_native_profiles() {
    let scratch = Scratch::new();
    for (name, collections, peak) in [
        ("memory_release_normal.cg", 2, 180),
        ("memory_release_nested.cg", 0, 180),
        ("memory_release_reserve_failure.cg", 0, 184),
    ] {
        let source_text = fs::read_to_string(source(name)).unwrap();
        let (result, Some(interpreter)) = run_checked_source(&source_text, &[]).unwrap() else {
            panic!("missing interpreter report: {name}");
        };
        assert_eq!(result.value, Value::Unit);
        assert_eq!(interpreter.totals.collection_count, collections, "{name}");
        assert_eq!(interpreter.totals.peak_bytes, peak, "{name}");
        assert_eq!(interpreter.totals.committed_bytes, 0, "{name}");
        for release in [false, true] {
            let (output, native) = build_and_run(name, release, None, &scratch);
            assert_eq!(output.status.code(), Some(0), "{name}: {:?}", output);
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{name}"
            );
            assert!(native.contains("\"status\":\"ok\""), "{name}");
            assert_eq!(
                totals(&native),
                report_totals(&interpreter),
                "{name}, release={release}"
            );
            assert!(
                native.contains("\"is_open\":false,\"mode\":\"manual\""),
                "{name}"
            );
        }
    }
}

#[test]
fn budget_and_host_reserve_failures_preserve_first_error_and_unwind_roots() {
    let scratch = Scratch::new();
    for (name, fault, detail, roots, committed) in [
        (
            "memory_release_budget_failure.cg",
            None,
            "BudgetExceeded",
            1,
            72,
        ),
        (
            "memory_release_reserve_failure.cg",
            Some(4),
            "HostAllocation",
            1,
            72,
        ),
        (
            "memory_release_reserve_failure.cg",
            Some(5),
            "HostAllocation",
            1,
            72,
        ),
    ] {
        let source_text = fs::read_to_string(source(name)).unwrap();
        let failure = match fault {
            Some(index) => run_checked_source_with_failure_at_reserve(&source_text, &[], index),
            None => run_checked_source_with_failure(&source_text, &[]),
        }
        .expect_err("expected failure")
        .remove(0);
        assert_eq!(failure.error.code, "CGR013", "{name}");
        assert!(failure.error.message.contains(detail), "{name}");
        assert_eq!(failure.rollback_preserved, Some(true), "{name}");
        assert!(failure.cleanup_errors.is_empty(), "{name}");
        let operation = failure.operation_failure.as_ref().unwrap();
        let final_unwind = failure.final_unwind.as_ref().unwrap();
        assert_eq!(operation.runtime_rooted_handles, Some(roots), "{name}");
        assert_eq!(operation.runtime_root_registrations, Some(roots), "{name}");
        assert_eq!(final_unwind.runtime_rooted_handles, Some(0), "{name}");
        assert_eq!(final_unwind.runtime_root_registrations, Some(0), "{name}");
        assert_eq!(operation.memory.totals.committed_bytes, committed, "{name}");
        assert_eq!(operation.memory.totals.collection_count, 0, "{name}");
        let span = failure.error.span.unwrap();
        for release in [false, true] {
            let (output, native) = build_and_run(name, release, fault, &scratch);
            assert_eq!(output.status.code(), Some(1), "{name}, release={release}");
            assert!(output.stdout.is_empty(), "{name}");
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(
                stderr.contains("CGR013") && stderr.contains(detail),
                "{name}: {stderr}"
            );
            assert!(
                stderr.contains(&format!("{}..{}", span.start, span.end)),
                "{name}: {stderr}"
            );
            assert!(
                native.contains("\"status\":\"error\",\"diagnostic_code\":\"CGR013\""),
                "{name}"
            );
            assert!(native.contains("\"rollback_preserved\":true"), "{name}");
            assert!(native.contains("\"cleanup_errors\":[]"), "{name}");
            assert_eq!(totals(&native), report_totals(&operation.memory), "{name}");
            let native_operation = native.split_once("\"operation_failure\":").unwrap().1;
            let native_final = native.split_once("\"final_unwind\":").unwrap().1;
            assert_eq!(
                field(native_operation, "runtime_rooted_handles"),
                roots as u64,
                "{name}"
            );
            assert_eq!(
                field(native_operation, "runtime_root_registrations"),
                roots as u64,
                "{name}"
            );
            assert_eq!(field(native_final, "runtime_rooted_handles"), 0, "{name}");
            assert_eq!(
                field(native_final, "runtime_root_registrations"),
                0,
                "{name}"
            );
            assert_eq!(totals(native_final)[0], committed as u64, "{name}");
        }
    }
}

#[test]
fn explicit_cross_region_copy_matches_native_and_escape_is_rejected() {
    let scratch = Scratch::new();
    let source_text = fs::read_to_string(source("memory_release_transfer.cg")).unwrap();
    let (result, Some(interpreter)) = run_checked_source(&source_text, &[]).unwrap() else {
        panic!("missing interpreter memory report");
    };
    assert_eq!(result.value, Value::Unit);
    assert_eq!(interpreter.totals.committed_bytes, 0);
    assert_eq!(interpreter.totals.live_bytes, 0);
    assert_eq!(interpreter.totals.collection_count, 0);
    assert_eq!(interpreter.regions.len(), 3);
    assert!(interpreter.regions.iter().skip(1).all(|row| !row.is_open));
    for release in [false, true] {
        let (output, native) = build_and_run("memory_release_transfer.cg", release, None, &scratch);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        assert!(native.contains("\"status\":\"ok\""));
        assert_eq!(totals(&native), report_totals(&interpreter));
        let native_memory = native
            .trim_end()
            .split_once("\"memory\":")
            .unwrap()
            .1
            .strip_suffix('}')
            .unwrap();
        assert_eq!(
            without_session_ids(native_memory),
            without_session_ids(&interpreter.to_json()),
            "full region report mismatch, release={release}"
        );
        assert!(native.contains("\"is_open\":false,\"mode\":\"manual\""));
        assert!(native.contains("\"is_open\":false,\"mode\":\"auto\""));
    }

    let escape = source("memory_release_escape.cg");
    let source_text = fs::read_to_string(&escape).unwrap();
    let diagnostics = run_checked_source(&source_text, &[]).unwrap_err();
    assert_eq!(diagnostics[0].code, "CGO003");
    assert!(diagnostics[0].message.contains("live handles"));
    for release in [false, true] {
        let mut build = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
        build.args(["build", escape.to_str().unwrap(), "--out"]);
        build.arg(scratch.path(if release {
            "escape-release"
        } else {
            "escape-debug"
        }));
        if release {
            build.arg("--release");
        }
        let output = build.output().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("CGO003") && stderr.contains("live handles"),
            "{stderr}"
        );
    }
}
