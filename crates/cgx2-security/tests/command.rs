use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn run(source: &str, policy: &str, format: &str) -> std::process::Output {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("cgx2-security-{}-{n}", std::process::id()));
    let source_path = base.with_extension("cg");
    let policy_path = base.with_extension("json");
    fs::write(&source_path, source).unwrap();
    fs::write(&policy_path, policy).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2-security"))
        .arg("audit")
        .arg(&source_path)
        .arg("--policy")
        .arg(&policy_path)
        .arg("--format")
        .arg(format)
        .output()
        .unwrap();
    fs::remove_file(source_path).unwrap();
    fs::remove_file(policy_path).unwrap();
    output
}

#[test]
fn exit_codes_distinguish_clean_findings_and_bad_input() {
    let policy = r#"{"schema_version":1,"allowed_effects":[],"allow_unknown":false}"#;
    let clean = run("let x = 1\n", policy, "json");
    assert_eq!(clean.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&clean.stdout).unwrap()["findings"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let finding = run("mystery()\n", policy, "json");
    assert_eq!(finding.status.code(), Some(1));
    assert!(String::from_utf8(finding.stdout)
        .unwrap()
        .contains("CGSEC002"));
    let malformed = run("let x = 1\n", "{}", "json");
    assert_eq!(malformed.status.code(), Some(2));
    assert!(String::from_utf8(malformed.stderr)
        .unwrap()
        .contains("CGSEC-POLICY"));
    let parse = run("fn bad(:\n", policy, "json");
    assert_eq!(parse.status.code(), Some(2));
    assert!(String::from_utf8(parse.stderr)
        .unwrap()
        .contains("CGSEC-INPUT"));
}

#[test]
fn sarif_command_is_deterministic_and_parseable() {
    let policy =
        r#"{"schema_version":1,"allowed_effects":["Foreign","ReadArgs"],"allow_unknown":true}"#;
    let source = "python.call(\"exec\", arg(0))\n";
    let first = run(source, policy, "sarif");
    let second = run(source, policy, "sarif");
    assert_eq!(first.status.code(), Some(1));
    // Temporary file names differ; compare structured results after removing URIs.
    let mut a: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let mut b: serde_json::Value = serde_json::from_slice(&second.stdout).unwrap();
    a["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"] =
        serde_json::Value::Null;
    b["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"] =
        serde_json::Value::Null;
    a["runs"][0]["results"][0]["relatedLocations"][0]["physicalLocation"]["artifactLocation"]
        ["uri"] = serde_json::Value::Null;
    b["runs"][0]["results"][0]["relatedLocations"][0]["physicalLocation"]["artifactLocation"]
        ["uri"] = serde_json::Value::Null;
    assert_eq!(a, b);
}
