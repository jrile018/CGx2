use cgx2_cli::{run_checked_source_with_failure, run_checked_source_with_failure_at_reserve};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-failure-parity-{}-{}",
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
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
fn build(
    source: &Path,
    binary: &Path,
    release: bool,
    fault: Option<usize>,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
    command.args([
        "build",
        source.to_str().unwrap(),
        "--out",
        binary.to_str().unwrap(),
    ]);
    if release {
        command.arg("--release");
    }
    if let Some(index) = fault {
        command.args(["--test-fail-reserve", &index.to_string()]);
    }
    command.output().unwrap()
}
fn number(json: &str, name: &str) -> u64 {
    let key = format!("\"{name}\":");
    let tail = json
        .split_once(&key)
        .unwrap_or_else(|| panic!("missing {name}"))
        .1;
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap()
}
fn semantic_json(json: &str) -> (Vec<Vec<u64>>, Vec<u64>) {
    let memory = json
        .split_once("\"memory\":")
        .map_or(json, |(_, tail)| tail);
    let rows = memory
        .split_once("\"regions\":[")
        .unwrap()
        .1
        .split_once("],\"totals\":")
        .unwrap()
        .0;
    let regions = if rows.is_empty() {
        Vec::new()
    } else {
        rows.split("},{\"region_id\"")
            .map(|row| {
                let mut fields = vec![
                    number(row, "index"),
                    u64::from(row.contains("\"is_open\":true")),
                ];
                for key in [
                    "limit_bytes",
                    "committed_bytes",
                    "live_bytes",
                    "peak_bytes",
                    "allocation_failures",
                    "collection_count",
                    "total_collected_bytes",
                ] {
                    fields.push(number(row, key));
                }
                fields
            })
            .collect()
    };
    let totals = memory.split_once("\"totals\":").unwrap().1;
    let totals = [
        "committed_bytes",
        "live_bytes",
        "peak_bytes",
        "allocation_failures",
        "collection_count",
        "total_collected_bytes",
    ]
    .iter()
    .map(|key| number(totals, key))
    .collect();
    (regions, totals)
}
fn compare_failure(
    name: &str,
    code: &str,
    detail: &str,
    fault: Option<usize>,
    roots: u64,
    edges: u64,
) {
    let case = Scratch::new();
    let source = fixture(name);
    let text = fs::read_to_string(&source).unwrap();
    let failure = match fault {
        Some(index) => run_checked_source_with_failure_at_reserve(&text, &[], index),
        None => run_checked_source_with_failure(&text, &[]),
    }
    .expect_err("interpreter must fail")
    .remove(0);
    assert_eq!(failure.error.code, code, "{name}");
    assert!(
        failure.error.message.contains(detail),
        "{name}: {}",
        failure.error.message
    );
    let span = failure.error.span.expect("source span");
    assert!(text.is_char_boundary(span.start) && text.is_char_boundary(span.end));
    assert_eq!(failure.rollback_preserved, Some(true), "{name}");
    let operation = failure
        .operation_failure
        .as_ref()
        .expect("operation snapshot");
    let final_unwind = failure.final_unwind.as_ref().expect("final snapshot");
    assert_eq!(operation.rooted_handles, Some(roots as usize), "{name}");
    assert_eq!(operation.rooted_edges, Some(edges as usize), "{name}");
    assert_eq!(final_unwind.rooted_handles, Some(0), "{name}");
    assert_eq!(final_unwind.rooted_edges, Some(0), "{name}");
    let interpreted = failure.report.expect("partial report").to_json();
    for release in [false, true] {
        let binary = case.path(&format!("{name}-{release}"));
        let report_path = case.path(&format!("{name}-{release}.json"));
        let built = build(&source, &binary, release, fault);
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let output = Command::new(&binary)
            .env("CGX2_MEMORY_REPORT", &report_path)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{name}: {stderr}");
        assert!(output.stdout.is_empty(), "{name}");
        assert!(
            stderr.contains(code) && stderr.contains(detail),
            "{name}: {stderr}"
        );
        assert!(
            stderr.contains(&format!("{}..{}", span.start, span.end)),
            "{name}: {stderr}"
        );
        let native = fs::read_to_string(&report_path).unwrap();
        let native_operation = native.split_once("\"operation_failure\":").unwrap().1;
        let native_final = native.split_once("\"final_unwind\":").unwrap().1;
        assert_eq!(number(native_operation, "rooted_handles"), roots, "{name}");
        assert_eq!(number(native_operation, "rooted_edges"), edges, "{name}");
        assert_eq!(number(native_final, "rooted_handles"), 0, "{name}");
        assert_eq!(number(native_final, "rooted_edges"), 0, "{name}");
        assert_eq!(
            semantic_json(native_operation),
            semantic_json(&operation.memory.to_json()),
            "{name} operation release={release}"
        );
        assert_eq!(
            semantic_json(native_final),
            semantic_json(&final_unwind.memory.to_json()),
            "{name} unwind release={release}"
        );
        assert!(native.contains("\"status\":\"error\""), "{name}");
        assert!(
            native.contains(&format!("\"diagnostic_code\":\"{code}\"")),
            "{name}"
        );
        assert!(
            native.contains(&format!("\"file\":\"{}\"", source.display())),
            "{name}"
        );
        assert!(
            native.contains(&format!(
                "\"span\":{{\"start\":{},\"end\":{}}}",
                span.start, span.end
            )),
            "{name}"
        );
        assert!(native.contains("\"rollback_preserved\":true"), "{name}");
        assert_eq!(
            semantic_json(&native),
            semantic_json(&interpreted),
            "{name} release={release}"
        );
    }
}

#[test]
fn source_runtime_failures_match_native_debug_and_release() {
    compare_failure(
        "reusable_memory_negative.cg",
        "CGR013",
        "byte size is negative",
        None,
        0,
        0,
    );
    compare_failure(
        "reusable_memory_region_failure.cg",
        "CGR013",
        "RegionLimit",
        None,
        0,
        0,
    );
    compare_failure(
        "reusable_memory_budget_failure.cg",
        "CGR013",
        "BudgetExceeded",
        None,
        0,
        0,
    );
    compare_failure(
        "reusable_memory_i64_max.cg",
        "CGR013",
        "RegionLimit",
        None,
        0,
        0,
    );
    compare_failure(
        "source_graph_dangling.cg",
        "CGR014",
        "InvalidHandle",
        None,
        1,
        0,
    );
    compare_failure(
        "failure_manual_limit.cg",
        "CGR013",
        "RegionLimit",
        None,
        2,
        1,
    );
    compare_failure(
        "failure_no_gc_budget.cg",
        "CGR013",
        "BudgetExceeded",
        None,
        2,
        1,
    );
    compare_failure(
        "failure_host_fault.cg",
        "CGR013",
        "HostAllocation",
        Some(6),
        2,
        1,
    );
    compare_failure(
        "failure_nested_mutation_limit.cg",
        "CGR013",
        "RegionLimit",
        None,
        3,
        1,
    );
}

#[cfg(target_os = "linux")]
#[test]
fn report_write_failure_keeps_primary_runtime_diagnostic() {
    let case = Scratch::new();
    let source = fixture("failure_manual_limit.cg");
    let binary = case.path("report-write-failure");
    let built = build(&source, &binary, false, None);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let output = Command::new(&binary)
        .env("CGX2_MEMORY_REPORT", "/proc/cgx2-failure-report.json")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("CGR013") && stderr.contains("RegionLimit"),
        "{stderr}"
    );
    assert!(stderr.contains("CGR017"), "{stderr}");
}

#[test]
fn statically_rejected_graph_failures_remain_source_located_and_unbuilt() {
    for (name, detail) in [
        (
            "failure_invalid_slot.cg",
            "reference index outside object shape",
        ),
        (
            "failure_cross_region.cg",
            "reference edge must stay in one region",
        ),
    ] {
        let case = Scratch::new();
        let source = fixture(name);
        let text = fs::read_to_string(&source).unwrap();
        let error = run_checked_source_with_failure(&text, &[])
            .err()
            .unwrap()
            .remove(0);
        assert_eq!(error.error.code, "CGM002");
        assert!(error.error.message.contains(detail));
        assert!(error.report.is_none());
        let span = error.error.span.unwrap();
        for release in [false, true] {
            let binary = case.path(&format!("{name}-{release}"));
            let output = build(&source, &binary, release, None);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{name}");
            assert!(
                stderr.contains("CGM002") && stderr.contains(detail),
                "{stderr}"
            );
            assert!(
                stderr.contains(&format!("{}..{}", span.start, span.end)),
                "{stderr}"
            );
            assert!(!binary.exists(), "{name}");
        }
    }
}

#[test]
fn fault_option_is_explicit_and_absent_from_ordinary_release_build() {
    let case = Scratch::new();
    let source = fixture("failure_host_fault.cg");
    let binary = case.path("ordinary-release");
    assert!(build(&source, &binary, true, None).status.success());
    let report = case.path("ordinary-release.json");
    let output = Command::new(&binary)
        .env("CGX2_MEMORY_REPORT", &report)
        .env("CGX2_TEST_FAIL_RESERVE", "6")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data = fs::read_to_string(report).unwrap();
    assert!(data.contains("\"status\":\"ok\""));
    assert!(data.contains("\"diagnostic_code\":null"));
    let emitted = case.path("emit.rs");
    let rejected = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
        .args([
            "emit-rust",
            source.to_str().unwrap(),
            "--out",
            emitted.to_str().unwrap(),
            "--test-fail-reserve",
            "6",
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(!emitted.exists());
    let scalar = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hello.cg");
    let scalar_binary = case.path("scalar");
    let rejected = build(&scalar, &scalar_binary, true, Some(1));
    assert!(!rejected.status.success());
    assert!(!scalar_binary.exists());
    let package_binary = case.path("package");
    let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/package-three");
    let rejected = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
        .args([
            "build",
            "--package",
            package.to_str().unwrap(),
            "--out",
            package_binary.to_str().unwrap(),
            "--test-fail-reserve",
            "1",
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(!package_binary.exists());
}
