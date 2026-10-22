use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Source(PathBuf);

impl Source {
    fn new(text: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-source-contract-{}-{}.cg",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, text).unwrap();
        Self(path)
    }

    fn run(&self, args: &[&str]) -> Value {
        let output = Command::new(env!("CARGO_BIN_EXE_cgx2-verify"))
            .arg(&self.0)
            .args(["--format", "json"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

const TRUE: &str = include_str!("../examples/finite.cg");
const FALSE: &str = include_str!("../examples/false.cg");

#[test]
fn finite_proof_and_re_evaluated_counterexample() {
    let valid = Source::new(TRUE).run(&["--domain", "x:0:2"]);
    assert_eq!(valid["schema_version"], 3);
    assert_eq!(valid["verdict"]["kind"], "proved");
    assert_eq!(valid["verdict"]["meaning"], "finite_domain_only");
    assert_eq!(valid["verdict"]["checked_cases"], 3);
    assert_eq!(valid["proof"]["kind"], "unknown");
    assert_eq!(valid["verdict"]["scope"]["domains"][0]["min"], 0);
    let false_contract = Source::new(FALSE).run(&["--domain", "x:0:2"]);
    assert_eq!(false_contract["verdict"]["kind"], "disproved");
    assert_eq!(false_contract["verdict"]["counterexample"]["x"], 0);
    assert_eq!(false_contract["verdict"]["failure"], "EnsuresFalse");
    assert_eq!(false_contract["proof"]["kind"], "disproved");
}

#[test]
fn boolean_parameter_is_enumerated_without_domain_flag() {
    let source = Source::new(
        "pub fn identity(flag: Bool) -> Bool effects() caps():\n    requires true\n    ensures result == flag\n    flag\n",
    );
    let result = source.run(&[]);
    assert_eq!(result["verdict"]["kind"], "proved");
    assert_eq!(result["verdict"]["checked_cases"], 2);
    assert_eq!(result["verdict"]["scope"]["boolean_parameters"][0], "flag");
}

#[test]
fn checked_body_overflow_is_a_failure_even_without_result_in_ensures() {
    let source = Source::new("pub fn next(x: i64) -> i64 effects() caps():\n    requires true\n    ensures true\n    x + 1\n");
    let result = source.run(&["--domain", "x:9223372036854775807:9223372036854775807"]);
    assert_eq!(result["verdict"]["kind"], "disproved");
    assert_eq!(result["verdict"]["failure"], "Overflow");
}

#[test]
fn inconsistent_and_undefined_preconditions_remain_unknown() {
    let inconsistent = Source::new("pub fn f(x: i64) -> i64 effects() caps():\n    requires x < 0\n    ensures result == x\n    x\n");
    let result = inconsistent.run(&["--domain", "x:0:1"]);
    assert_eq!(result["verdict"]["kind"], "unknown");
    assert!(result["verdict"]["reason"]
        .as_str()
        .unwrap()
        .contains("InconsistentPrecondition"));
    let undefined = Source::new("pub fn f(x: i64) -> i64 effects() caps():\n    requires x + 1 > 0\n    ensures result == x\n    x\n");
    let result = undefined.run(&["--domain", "x:9223372036854775807:9223372036854775807"]);
    assert_eq!(result["verdict"]["kind"], "unknown");
    assert!(result["verdict"]["reason"]
        .as_str()
        .unwrap()
        .contains("Evaluation"));
}

#[test]
fn domains_and_function_selection_are_explicit() {
    let source = Source::new(TRUE);
    assert_eq!(source.run(&[])["verdict"]["kind"], "unknown");
    assert_eq!(
        source.run(&["--domain", "x:0:2", "--max-cases", "1"])["verdict"]["kind"],
        "unknown"
    );
    let two = Source::new(&format!("{TRUE}\n{}", TRUE.replace("next", "other")));
    assert_eq!(
        two.run(&["--domain", "x:0:2"])["verdict"]["kind"],
        "unknown"
    );
    let selected = two.run(&["--function", "other", "--domain", "x:0:2"]);
    assert_eq!(selected["source"]["function"], "other");
    assert_eq!(selected["verdict"]["kind"], "proved");
}

#[test]
fn unsupported_calls_effects_and_foreign_declarations_have_source_spans() {
    for text in [
        "pub fn f(x: i64) -> i64 effects() caps():\n    requires true\n    ensures result == x\n    missing(x)\n",
        "pub fn f(x: i64) -> i64 effects(ReadArgs) caps(ArgsRead):\n    requires true\n    ensures result == x\n    x\n",
        "foreign python fn f(x: i64) -> i64 effects(Foreign) caps(ForeignCall) math.isqrt runtime \"local:cpython\" ownership copied\n",
        "pub fn f(x: i64) -> i64 effects() caps():\n    requires true\n    ensures result == x\n    x * 2\n",
    ] {
        let result = Source::new(text).run(&["--domain", "x:0:1"]);
        assert_eq!(result["verdict"]["kind"], "unknown", "{result}");
        let diagnostic = &result["verdict"]["diagnostic"];
        assert!(diagnostic["span"]["end"].as_u64().unwrap() > diagnostic["span"]["start"].as_u64().unwrap(), "{result}");
        assert_eq!(result["source"]["span"], diagnostic["span"]);
        assert_eq!(result["verdict"]["scope"]["max_cases"], 100_000);
        assert_eq!(result["verdict"]["scope"]["requested_domains"][0]["name"], "x");
        assert_eq!(result["proof"]["kind"], "unknown");
    }
}

#[test]
fn changed_source_changes_digest() {
    let source = Source::new(TRUE);
    let first = source.run(&["--domain", "x:0:2"]);
    fs::write(&source.0, FALSE).unwrap();
    let second = source.run(&["--domain", "x:0:2"]);
    assert_ne!(first["input_sha256"], second["input_sha256"]);
    assert_eq!(second["verdict"]["kind"], "disproved");
}
