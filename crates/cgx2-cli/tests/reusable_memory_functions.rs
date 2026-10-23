use std::{path::PathBuf, process::Command};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../cgx2c/tests/fixtures")
        .join(name)
}

#[test]
fn checked_scalar_memory_calls_run_and_expose_public_api_json() {
    let source = fixture("reusable_memory_success.cg");
    for mode in ["check", "run"] {
        let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
            .arg(mode)
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["api", source.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let api: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let function = &api["functions"][0];
    assert_eq!(function["name"], "allocate");
    assert_eq!(function["parameters"][0]["name"], "size");
    assert_eq!(function["parameters"][0]["ty"], "i64");
    assert_eq!(function["return_type"], "Unit");
    assert_eq!(
        function["effects"],
        serde_json::json!(["ManagedAlloc", "Mutate"])
    );
}

#[test]
fn dynamic_size_failures_have_source_spans_in_cli() {
    for name in [
        "reusable_memory_negative.cg",
        "reusable_memory_region_failure.cg",
        "reusable_memory_budget_failure.cg",
        "reusable_memory_i64_max.cg",
    ] {
        let source = fixture(name);
        let check = Command::new(env!("CARGO_BIN_EXE_cgx2"))
            .args(["check", source.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&check.stderr)
        );
        let run = Command::new(env!("CARGO_BIN_EXE_cgx2"))
            .args(["run", source.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(!run.status.success(), "{name}");
        let stderr = String::from_utf8(run.stderr).unwrap();
        assert!(
            stderr.contains("CGR013") && stderr.contains(".."),
            "{name}: {stderr}"
        );
    }
}
