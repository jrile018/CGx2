use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

fn run(source: &str, extra: &[&str]) -> std::process::Output {
    let path = std::env::temp_dir().join(format!(
        "cgx2-effect-index-{}-{}.cg",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2-effect-index"))
        .arg(&path)
        .args(extra)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    output
}

#[test]
fn json_command_is_byte_identical_across_runs() {
    let source = "fn z(x: i64) -> i64:\n    x\nfn a(x: i64) -> i64:\n    z(x)\n";
    let first = run(source, &["--format", "json"]);
    let second = run(source, &["--format", "json"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let json = String::from_utf8(first.stdout).unwrap();
    assert!(json.find("\"name\":\"a\"").unwrap() < json.find("\"name\":\"z\"").unwrap());
    assert!(json.contains("\"span_unit\":\"utf8_byte_offset\""));
}

#[test]
fn command_reports_check_failure_and_explicit_fallback() {
    let source = "mystery()\n";
    let failed = run(source, &[]);
    assert!(!failed.status.success());
    let stderr = String::from_utf8(failed.stderr).unwrap();
    assert!(stderr.contains("CGT001 check"));
    assert!(stderr.contains(":1:1 [0..7)"));
    let fallback = run(source, &["--syntax-only", "--format", "json"]);
    assert!(fallback.status.success());
    let json = String::from_utf8(fallback.stdout).unwrap();
    assert!(json.contains("\"check_mode\":\"syntax_only\""));
    assert!(json.contains("\"direct_effects\":[\"Unknown\"]"));
}
