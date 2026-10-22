use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf);

impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-source-graph-{}-{}",
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

fn command(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cgx2c"))
        .args(args)
        .output()
        .unwrap()
}

fn rustc(args: &[&str]) {
    let output = Command::new("rustc").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn emitted_graph_source_runs_and_rejects_dangling_edges_in_both_profiles() {
    let case = Case::new();
    let runtime_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cgx2-runtime/src/lib.rs");
    let runtime = case.path("libcgx2_runtime.rlib");
    rustc(&[
        "--edition=2021",
        "--crate-name",
        "cgx2_runtime",
        "--crate-type=lib",
        runtime_source.to_str().unwrap(),
        "-o",
        runtime.to_str().unwrap(),
    ]);
    for (fixture_name, expected_code) in [
        ("source_graph_cycle.cg", None),
        ("source_graph_dangling.cg", Some("CGR014")),
    ] {
        let source = fixture(fixture_name);
        let emitted = case.path(&format!("{fixture_name}.rs"));
        let output = command(&[
            "emit-rust",
            source.to_str().unwrap(),
            "--out",
            emitted.to_str().unwrap(),
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for (profile, opt) in [("debug", "0"), ("release", "3")] {
            let binary = case.path(&format!("{fixture_name}-{profile}"));
            rustc(&[
                "--edition=2024",
                "--crate-name",
                "cgx2_generated",
                emitted.to_str().unwrap(),
                "--extern",
                &format!("cgx2_runtime={}", runtime.display()),
                "-C",
                &format!("opt-level={opt}"),
                "-o",
                binary.to_str().unwrap(),
            ]);
            let report = case.path(&format!("{fixture_name}-{profile}.json"));
            let result = Command::new(&binary)
                .env("CGX2_MEMORY_REPORT", &report)
                .output()
                .unwrap();
            let json = fs::read_to_string(report).unwrap();
            assert_eq!(result.status.success(), expected_code.is_none());
            if let Some(code) = expected_code {
                assert!(String::from_utf8_lossy(&result.stderr).contains(code));
                assert!(json.contains(&format!("\"diagnostic_code\":\"{code}\"")));
            } else {
                assert!(result.stderr.is_empty());
                assert!(json.contains("\"status\":\"ok\""));
            }
        }
    }
}

#[test]
fn package_entry_rejects_checked_graph_source_for_build_and_emit() {
    let case = Case::new();
    let entry = case.path("main.cg");
    let body = include_str!("fixtures/source_graph_cycle.cg");
    let source = format!(
        "module demo.main\nfn main() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n{}",
        body.lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>()
    );
    check_module(&parse_module(&source).unwrap()).unwrap();
    fs::write(&entry, source).unwrap();
    fs::write(case.path("cgx2.toml"), "schema_version = 1\nname = \"graph_package\"\nentry = \"main.cg\"\nsources = [\"main.cg\"]\n").unwrap();
    for mode in ["build", "emit-rust"] {
        let output_path = case.path(&format!("{mode}.out"));
        let output = command(&[
            mode,
            "--package",
            case.0.to_str().unwrap(),
            "--out",
            output_path.to_str().unwrap(),
        ]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(stderr.contains("CGRS001"), "{stderr}");
        assert!(stderr.contains("main.cg:"), "{stderr}");
        assert!(!output_path.exists());
    }
}

#[test]
fn injected_graph_allocation_failure_has_source_span_and_no_collection() {
    let case = Case::new();
    let runtime_source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cgx2-runtime/src/lib.rs");
    let runtime = case.path("libcgx2_runtime.rlib");
    rustc(&[
        "--edition=2021",
        "--crate-name",
        "cgx2_runtime",
        "--crate-type=lib",
        "--cfg",
        "feature=\"fault-injection\"",
        runtime_source.to_str().unwrap(),
        "-o",
        runtime.to_str().unwrap(),
    ]);
    let source = fixture("source_graph_cycle.cg");
    let emitted = case.path("graph.rs");
    let output = command(&[
        "emit-rust",
        source.to_str().unwrap(),
        "--out",
        emitted.to_str().unwrap(),
    ]);
    assert!(output.status.success());
    let generated = fs::read_to_string(&emitted).unwrap();
    let needle = "let handle = session.alloc_object(id, payload, slots)";
    assert!(generated.contains(needle));
    // Enable the frozen runtime's conformance hook at the graph allocation site.
    let instrumented = generated.replacen(
        needle,
        &format!("session.inject_host_failure_at_reserve(1);\n    {needle}"),
        1,
    );
    fs::write(&emitted, instrumented).unwrap();
    let module = parse_module(&fs::read_to_string(&source).unwrap()).unwrap();
    let span = module.statements[1].span;
    for (profile, opt) in [("debug", "0"), ("release", "3")] {
        let binary = case.path(&format!("injected-{profile}"));
        rustc(&[
            "--edition=2024",
            "--crate-name",
            "cgx2_generated",
            emitted.to_str().unwrap(),
            "--extern",
            &format!("cgx2_runtime={}", runtime.display()),
            "-C",
            &format!("opt-level={opt}"),
            "-o",
            binary.to_str().unwrap(),
        ]);
        let report = case.path(&format!("injected-{profile}.json"));
        let result = Command::new(binary)
            .env("CGX2_MEMORY_REPORT", &report)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        let json = fs::read_to_string(report).unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(stderr.contains("CGR013"), "{stderr}");
        assert!(stderr.contains("HostAllocation"), "{stderr}");
        assert!(stderr.contains(&format!("{}..{}", span.start, span.end)));
        assert!(json.contains("\"diagnostic_code\":\"CGR013\""));
        assert!(json.contains("\"allocation_failures\":1"));
        assert!(json.contains("\"collection_count\":0"));
    }
}
