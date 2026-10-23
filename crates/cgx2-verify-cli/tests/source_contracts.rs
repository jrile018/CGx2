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
fn source_and_smt_parity_covers_i64_max_body_overflow() {
    let result = Source::new(TRUE).run(&["--domain", "x:9223372036854775806:9223372036854775807"]);
    assert_eq!(result["bounded_solver_result"]["kind"], "cross_checked");
    assert_eq!(result["bounded_solver_result"]["source_parity"], true);
    assert_eq!(
        result["solver_fixture_catalog"].as_array().unwrap().len(),
        3
    );
    assert!(result["solver_fixture_catalog"][2]["smtlib"]
        .as_str()
        .unwrap()
        .contains("; vc:ensures_overflow"));
    assert_eq!(result["bounded_solver_result"]["checked_cases"], 2);
    assert_eq!(
        result["bounded_solver_result"]["obligations"]["ensures_overflow"],
        1
    );
    assert_eq!(result["verdict"]["kind"], "disproved");
    assert_eq!(result["proof"]["kind"], "disproved");
    assert_eq!(result["proof"]["counterexample"]["x"], i64::MAX);
    assert_eq!(result["source"]["obligations"][1]["kind"], "body_defined");
}

#[cfg(unix)]
#[test]
fn solver_version_spoof_with_canned_statuses_cannot_prove() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!(
        "cgx2-solver-probe-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let solver = directory.join("z3");
    fs::write(&solver, "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Z3 version 4.12.2'; exit 0; fi\ncat >/dev/null\nprintf 'unsat\\nsat\\nunsat\\nunsat\\n'\n").unwrap();
    fs::set_permissions(&solver, fs::Permissions::from_mode(0o700)).unwrap();
    let result =
        Source::new(TRUE).run(&["--domain", "x:0:2", "--solver", solver.to_str().unwrap()]);
    assert_eq!(result["bounded_solver_result"]["source_parity"], true);
    assert_eq!(result["solver"]["identity_valid"], true);
    assert_eq!(result["solver"]["fixtures_valid"], false);
    assert_eq!(result["solver"]["bounded_reason"], "solver_fixture_failed");
    assert_eq!(result["proof"]["kind"], "unknown");
    fs::remove_file(solver).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn fixture_spoof_without_pin_and_stale_model_remain_untrusted() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!(
        "cgx2-solver-stale-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let solver = directory.join("z3");
    let calls = directory.join("calls");
    let _ = fs::remove_file(&calls);
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'Z3 version 4.13.0'; exit 0; fi\ncat >/dev/null\nn=0\nif [ -r '{calls}' ]; then n=$(cat '{calls}'); fi\nn=$((n+1))\nprintf '%s' \"$n\" > '{calls}'\ncase \"$n\" in\n1) printf 'unsat\\nsat\\nsat\\nunsat\\n';;\n2) printf 'unsat\\nsat\\nunsat\\nunsat\\n';;\n3) printf 'unsat\\nsat\\nsat\\nunsat\\n';;\n4) printf 'unsat\\nsat\\nunsat\\nsat\\n';;\n5) printf 'sat\\n((v_78 0))\\n';;\nesac\n",
        calls = calls.display()
    );
    fs::write(&solver, script).unwrap();
    fs::set_permissions(&solver, fs::Permissions::from_mode(0o700)).unwrap();
    let result =
        Source::new(TRUE).run(&["--domain", "x:0:2", "--solver", solver.to_str().unwrap()]);
    assert_eq!(result["solver"]["fixtures_valid"], true, "{result}");
    assert_eq!(result["solver"]["bounded_reason"], "solver_digest_unpinned");
    assert_eq!(result["solver"]["response"], "sat_candidate_unvalidated");
    assert!(result["solver"]["witness_attempts"][0]["validation"]
        .as_str()
        .unwrap()
        .contains("candidate does not violate ensures"));
    assert_eq!(result["proof"]["kind"], "unknown");
    assert_eq!(result["proof"]["state"], "unknown");
    fs::remove_file(&calls).unwrap();
    let wrong_pin = "0".repeat(64);
    let mismatch = Source::new(TRUE).run(&[
        "--domain",
        "x:0:2",
        "--solver",
        solver.to_str().unwrap(),
        "--trusted-solver-sha256",
        &wrong_pin,
    ]);
    assert_eq!(
        mismatch["solver"]["bounded_reason"],
        "solver_digest_mismatch"
    );
    assert_eq!(mismatch["proof"]["state"], "solver_failure");
    fs::remove_dir_all(directory).unwrap();
}

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
    assert_eq!(selected["proof"]["kind"], "unknown");
    assert_eq!(selected["proof"]["state"], "unknown");
    assert_eq!(
        selected["proof"]["scope"]["kind"],
        "selected_source_function_contract"
    );
    assert_eq!(selected["proof"]["scope"]["function"], "other");
    assert_eq!(
        selected["proof"]["scope"]["input_sha256"],
        selected["input_sha256"]
    );
}

#[test]
fn selected_finite_result_does_not_cover_unsupported_sibling() {
    let source = format!(
        "{TRUE}\npub fn effectful(x: i64) -> i64 effects(ReadArgs) caps(ArgsRead):\n    requires true\n    ensures result == x\n    x\n"
    );
    let result = Source::new(&source).run(&["--function", "next", "--domain", "x:0:2"]);
    assert_eq!(result["verdict"]["kind"], "proved", "{result}");
    assert_eq!(result["proof"]["state"], "unknown");
    assert_eq!(
        result["proof"]["scope"]["kind"],
        "selected_source_function_contract"
    );
    assert_eq!(result["proof"]["scope"]["function"], "next");
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
        assert_eq!(result["proof"]["state"], "unsupported");
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

#[test]
fn source_artifact_names_obligations_and_rejects_unchecked_assert() {
    let valid = Source::new(TRUE).run(&["--domain", "x:0:2"]);
    let obligations = valid["source"]["obligations"].as_array().unwrap();
    assert_eq!(obligations.len(), 3);
    assert_eq!(obligations[0]["kind"], "requires_defined_and_satisfiable");
    assert_eq!(obligations[1]["kind"], "body_defined");
    assert_eq!(obligations[2]["kind"], "ensures");
    assert_eq!(
        valid["source"]["unsupported_obligations"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let text = "pub fn f(x: i64) -> i64 effects() caps():\n requires true\n ensures result == x\n assert(x == x)\n x\n";
    let result = Source::new(text).run(&["--domain", "x:0:1"]);
    assert_eq!(result["proof"]["kind"], "unknown");
    assert_eq!(result["verdict"]["kind"], "unknown");
    assert_eq!(
        result["unsupported_obligations"][0]["span"],
        result["verdict"]["diagnostic"]["span"]
    );
    let span = &result["verdict"]["diagnostic"]["span"];
    let start = span["start"].as_u64().unwrap() as usize;
    let end = span["end"].as_u64().unwrap() as usize;
    assert_eq!(text[start..end].trim_end(), "assert(x == x)");
}

/// Set CGX2_LIVE_SOLVER to an installed z3/cvc5 binary to run this gate.
/// Finite enumeration independently checks the same small Boolean domain.
#[test]
fn live_solver_boolean_crosscheck_when_configured() {
    let Ok(path) = std::env::var("CGX2_LIVE_SOLVER") else {
        return;
    };
    let valid = Source::new(
        "pub fn id(x: Bool) -> Bool effects() caps():\n requires true\n ensures result == x\n x\n",
    )
    .run(&["--solver", &path]);
    assert_eq!(valid["verdict"]["kind"], "proved", "{valid}");
    assert_eq!(valid["verdict"]["checked_cases"], 2);
    assert_eq!(
        valid["solver"]["bounded_result"], "unsat_observed",
        "{valid}"
    );
    assert_eq!(valid["proof"]["kind"], "unknown");

    let invalid = Source::new(
        "pub fn id(x: Bool) -> Bool effects() caps():\n requires true\n ensures result == false\n x\n",
    )
    .run(&["--solver", &path]);
    assert_eq!(invalid["verdict"]["kind"], "disproved", "{invalid}");
    assert_eq!(invalid["verdict"]["counterexample"]["x"], true);
    assert_eq!(
        invalid["solver"]["response"], "validated_counterexample",
        "{invalid}"
    );
}
