use serde_json::Value;
use std::{path::Path, process::Command};

#[test]
fn opt_in_failure_report_preserves_primary_diagnostic_and_success_output() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/object_stale_reference.cg");
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["memory", "failure-report", source.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("CGR014") && stderr.contains("InvalidHandle"),
        "{stderr}"
    );
    let failure: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(failure["status"], "error");
    assert_eq!(failure["diagnostic_code"], "CGR014");
    assert_eq!(failure["file"], source.to_string_lossy().as_ref());
    assert_eq!(failure["rollback_preserved"], true);
    assert!(
        failure["memory"]["totals"]["collection_count"]
            .as_u64()
            .unwrap()
            > 0
    );
    let start = failure["span"]["start"].as_u64().unwrap() as usize;
    let end = failure["span"]["end"].as_u64().unwrap() as usize;
    assert!(std::fs::read_to_string(&source).unwrap()[start..end].contains("memory.use_reference"));

    let success = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/object_references.cg");
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["memory", "failure-report", success.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let ordinary = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args([
            "memory",
            "report",
            success.to_str().unwrap(),
            "--format=json",
        ])
        .output()
        .unwrap();
    assert!(ordinary.status.success());
    let report: Value = serde_json::from_slice(&ordinary.stdout).unwrap();
    assert!(report["regions"].is_array());
    assert!(report.get("status").is_none());
}
