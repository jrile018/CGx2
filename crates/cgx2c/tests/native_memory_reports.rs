use cgx2_cli::{MemoryReport, run_checked_source};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf);

impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-native-report-{}-{}",
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

impl Drop for Case {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn build(source: &Path, output: &Path, release: bool) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
    command.args([
        "build",
        source.to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
    ]);
    if release {
        command.arg("--release");
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
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

fn region_fields(row: &str) -> Vec<u64> {
    let mut fields = vec![number(row, "index")];
    fields.push(u64::from(row.contains("\"is_open\":true")));
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
        rows.split("},{\"region_id\"").map(region_fields).collect()
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

fn semantic_interpreter(report: &MemoryReport) -> (Vec<Vec<u64>>, Vec<u64>) {
    semantic_json(&report.to_json())
}

#[test]
fn native_reports_match_interpreter_and_failure_counters_in_both_profiles() {
    let case = Case::new();
    let fixtures = [
        ("memory_functions.cg", None, 1, None),
        ("memory_functions_no_gc.cg", None, 0, None),
        (
            "memory_functions_nested_budget_failure.cg",
            Some("BudgetExceeded"),
            0,
            Some(256),
        ),
        (
            "memory_functions_region_failure.cg",
            Some("RegionLimit"),
            0,
            Some(64),
        ),
        (
            "memory_functions_conversion_failure.cg",
            Some("RegionLimit"),
            0,
            Some(64),
        ),
    ];
    for (name, failure, collections, failed_limit) in fixtures {
        let source = fixture(name);
        let interpreted = run_checked_source(&fs::read_to_string(&source).unwrap(), &[]);
        let mut first_native = None;
        for release in [false, true] {
            let binary = case.path(&format!("{name}-{release}"));
            let report_path = case.path(&format!("{name}-{release}.json"));
            build(&source, &binary, release);
            let output = Command::new(&binary)
                .env("CGX2_MEMORY_REPORT", &report_path)
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(if failure.is_some() { 1 } else { 0 })
            );
            assert!(output.stdout.is_empty());
            let json = fs::read_to_string(&report_path).unwrap();
            assert!(json.len() <= 1024 * 1024);
            assert!(json.contains("\"native_report_schema_version\":1"));
            assert!(json.contains("\"schema_version\":1"));
            let native = semantic_json(&json);
            assert_eq!(native.1[4], collections, "{name}");
            if let Some(expected) = failure {
                let error = &interpreted.as_ref().unwrap_err()[0];
                assert_eq!(error.code, "CGR013");
                assert!(error.message.contains(expected));
                assert!(json.contains("\"status\":\"error\",\"diagnostic_code\":\"CGR013\""));
                assert_eq!(native.0.len(), 2, "{name}: default and named region");
                assert_eq!(native.0[0][6], 0, "{name}: default region has no failure");
                assert_eq!(
                    native.0[1][2],
                    failed_limit.unwrap(),
                    "{name}: region limit"
                );
                assert_eq!(native.0[1][4], 0, "{name}: no committed live bytes");
                assert_eq!(native.0[1][5], 0, "{name}: no prior peak bytes");
                assert_eq!(native.0[1][6], 1, "{name}: one failed allocation");
                assert_eq!(native.1[3], 1, "{name}: one failed allocation in totals");
                assert!(
                    native.0.iter().all(|row| row[7] == 0),
                    "{name}: no collection on failure"
                );
            } else {
                let (_, Some(interpreter_report)) = interpreted.as_ref().unwrap() else {
                    panic!("missing interpreter report");
                };
                assert!(json.contains("\"status\":\"ok\",\"diagnostic_code\":null"));
                assert_eq!(native, semantic_interpreter(interpreter_report), "{name}");
            }
            if let Some(previous) = &first_native {
                assert_eq!(&native, previous, "debug/release differ for {name}");
            }
            first_native = Some(native);
        }
    }
}

#[test]
fn invalid_report_paths_leave_binary_and_existing_output_intact() {
    let case = Case::new();
    let binary = case.path("program");
    build(&fixture("memory_functions.cg"), &binary, false);
    let existing = case.path("existing.json");
    fs::write(&existing, "previous report").unwrap();
    for invalid in [&binary, &case.0, &case.path("missing/next.json")] {
        let output = Command::new(&binary)
            .env("CGX2_MEMORY_REPORT", invalid)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("CGR017"));
        assert!(output.stdout.is_empty());
    }
    assert_eq!(fs::read_to_string(&existing).unwrap(), "previous report");
    assert!(Command::new(&binary).output().unwrap().status.success());
    assert!(!case.path("missing/next.json").exists());
    let replaced = Command::new(&binary)
        .env("CGX2_MEMORY_REPORT", &existing)
        .output()
        .unwrap();
    assert!(replaced.status.success());
    assert!(
        fs::read_to_string(&existing)
            .unwrap()
            .contains("\"status\":\"ok\"")
    );

    let original_binary = fs::read(&binary).unwrap();
    let absent_report = case.path("failed-build.json");
    let failed = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
        .args([
            "build",
            fixture("memory_functions_dynamic_invalid.cg")
                .to_str()
                .unwrap(),
            "--out",
            binary.to_str().unwrap(),
            "--force",
        ])
        .env("CGX2_MEMORY_REPORT", &absent_report)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(!absent_report.exists());
    assert_eq!(fs::read(binary).unwrap(), original_binary);
}
