use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(bytes: &[u8], suffix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-verify-test-{}-{}-{suffix}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, bytes).unwrap();
        Self { path }
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn run(input: &Temp, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cgx2-verify"))
        .arg(&input.path)
        .args(args)
        .output()
        .unwrap()
}

fn parsed(input: &Temp, args: &[&str]) -> Value {
    let output = run(input, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fixture(text: &str) -> Temp {
    Temp::new(text.as_bytes(), "contract.json")
}

#[test]
fn finite_proof_scope_and_stable_json() {
    let input = fixture(include_str!("../examples/valid.json"));
    let args = ["--format", "json"];
    let first = run(&input, &args);
    let second = run(&input, &args);
    assert_eq!(first.stdout, second.stdout);
    let result: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(result["schema_version"], 2);
    assert_eq!(result["input_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(result["verdict"]["kind"], "proved");
    assert_eq!(result["verdict"]["meaning"], "finite_domain_only");
    assert_eq!(result["verdict"]["checked_cases"], 6);
    assert_eq!(result["verdict"]["scope"]["domains"][0]["min"], -2);
    assert_eq!(result["verdict"]["scope"]["domains"][0]["max"], 3);
}

#[test]
fn false_inconsistent_cap_and_foreign_call() {
    let false_case = fixture(include_str!("../examples/false.json"));
    let result = parsed(&false_case, &["--format", "json"]);
    assert_eq!(result["verdict"]["kind"], "disproved");
    assert_eq!(result["verdict"]["counterexample"]["x"], -2);
    assert_eq!(result["verdict"]["failure"], "EnsuresFalse");
    let inconsistent = fixture(include_str!("../examples/inconsistent.json"));
    assert_eq!(
        parsed(&inconsistent, &["--format", "json"])["verdict"]["reason"],
        "InconsistentPrecondition"
    );
    assert!(
        parsed(&inconsistent, &["--format", "json", "--max-cases", "1"])["verdict"]["reason"]
            .as_str()
            .unwrap()
            .contains("CaseCap")
    );
    let unsupported = fixture(include_str!("../examples/unsupported.json"));
    assert_eq!(
        parsed(&unsupported, &["--format", "json"])["verdict"]["kind"],
        "unknown"
    );
    assert!(
        parsed(&unsupported, &["--format", "json"])["verdict"]["reason"]
            .as_str()
            .unwrap()
            .contains("uninterpreted call")
    );
}

#[test]
fn malformed_oversized_and_invalid_domains_are_rejected() {
    for text in [
        "{",
        r#"{"version":2}"#,
        r#"{"version":1,"parameters":[],"domains":[],"requires":{"op":"mystery"},"ensures":{"op":"bool","value":true}}"#,
    ] {
        let input = fixture(text);
        assert_eq!(run(&input, &["--format", "json"]).status.code(), Some(2));
    }
    let oversized = fixture(&" ".repeat(65_537));
    assert_eq!(run(&oversized, &[]).status.code(), Some(2));
    let mut doc: Value = serde_json::from_str(include_str!("../examples/valid.json")).unwrap();
    doc["domains"][0]["min"] = json!(0.5);
    assert_eq!(run(&fixture(&doc.to_string()), &[]).status.code(), Some(2));
    doc["domains"][0]["min"] = json!(0);
    doc["domains"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"x","min":0,"max":1}));
    assert_eq!(run(&fixture(&doc.to_string()), &[]).status.code(), Some(2));
    doc["domains"] = json!([]);
    assert_eq!(run(&fixture(&doc.to_string()), &[]).status.code(), Some(2));
    doc["domains"] = json!([{"name":"y","min":0,"max":1}]);
    assert_eq!(run(&fixture(&doc.to_string()), &[]).status.code(), Some(2));
}

#[cfg(unix)]
fn fake_solver(body: &str) -> Temp {
    use std::os::unix::fs::PermissionsExt;
    // Consume the solver script before producing a response. A fake solver that
    // exits immediately races the parent's stdin writer and can spuriously
    // report a broken pipe under the full workspace test load.
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo fake-1; exit 0; fi\ncat >/dev/null\n{body}\n"
    );
    let file = Temp::new(script.as_bytes(), "solver.sh");
    fs::set_permissions(&file.path, fs::Permissions::from_mode(0o700)).unwrap();
    file
}

#[cfg(unix)]
#[test]
fn fake_solver_only_disproves_with_a_validated_model() {
    let input = fixture(include_str!("../examples/valid.json"));
    let valid = fake_solver("printf 'unsat\\nsat\\nsat\\nunsat\\n'");
    let result = parsed(
        &input,
        &["--format", "json", "--solver", valid.path.to_str().unwrap()],
    );
    assert_eq!(result["schema_version"], 2);
    assert_eq!(result["proof"]["kind"], "unknown");
    assert_eq!(result["solver"]["trust"], "untrusted_protocol");

    use std::os::unix::fs::PermissionsExt;
    let body = "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo fake-1; exit 0; fi\ninput=$(cat)\ncase \"$input\" in *get-value*) printf 'sat\\n((v_78 4))\\n';; *) printf 'unsat\\nsat\\nsat\\nunsat\\n';; esac\n";
    let solver = Temp::new(body.as_bytes(), "model-solver.sh");
    fs::set_permissions(&solver.path, fs::Permissions::from_mode(0o700)).unwrap();
    let result = parsed(
        &input,
        &[
            "--format",
            "json",
            "--solver",
            solver.path.to_str().unwrap(),
        ],
    );
    assert_eq!(result["verdict"]["meaning"], "finite_domain_only");
    assert_eq!(result["proof"]["kind"], "disproved", "{result}");
    assert_eq!(result["proof"]["meaning"], "validated_counterexample");
    assert_eq!(result["proof"]["counterexample"]["x"], 4);
    assert_eq!(result["solver"]["response"], "validated_counterexample");

    let invalid = body.replace("v_78 4", "v_78 0");
    let solver = Temp::new(invalid.as_bytes(), "invalid-model-solver.sh");
    fs::set_permissions(&solver.path, fs::Permissions::from_mode(0o700)).unwrap();
    let result = parsed(
        &input,
        &[
            "--format",
            "json",
            "--solver",
            solver.path.to_str().unwrap(),
        ],
    );
    assert_eq!(result["proof"]["kind"], "unknown");
    assert_eq!(result["solver"]["response"], "sat_candidate_unvalidated");
}

#[cfg(unix)]
#[test]
fn solver_statuses_are_untrusted_observations() {
    let input = fixture(include_str!("../examples/valid.json"));
    let cases = [
        (
            "printf 'unsat\\nsat\\nunsat\\nunsat\\n'",
            "unsat_protocol_only",
        ),
        (
            "printf 'unsat\\nsat\\nsat\\nunsat\\n'",
            "sat_candidate_unvalidated",
        ),
        (
            "printf 'unsat\\nsat\\nunknown\\nunsat\\n'",
            "unknown:SolverUnknown",
        ),
        ("printf 'junk\\n'", "unknown:MalformedResponse"),
        (
            "printf 'unsat\\nsat\\nunsat\\nunsat\\n'; exit 3",
            "unknown:MalformedResponse",
        ),
        (
            "printf 'error' >&2; printf 'unsat\\nsat\\nunsat\\nunsat\\n'",
            "unknown:MalformedResponse",
        ),
    ];
    for (body, expected) in cases {
        let solver = fake_solver(body);
        let result = parsed(
            &input,
            &[
                "--format",
                "json",
                "--solver",
                solver.path.to_str().unwrap(),
            ],
        );
        assert_eq!(result["verdict"]["kind"], "proved");
        assert_eq!(result["proof"]["kind"], "unknown");
        assert_eq!(result["solver"]["trust"], "untrusted_protocol");
        assert_eq!(result["solver"]["version"], "fake-1");
        assert_eq!(
            result["solver"]["script_sha256"].as_str().unwrap().len(),
            64
        );
        assert_eq!(result["solver"]["response"], expected);
    }
    let result = parsed(
        &input,
        &["--format", "json", "--solver", "/definitely/missing/solver"],
    );
    assert_eq!(result["solver"]["response"], "unknown:SolverUnavailable");
}

#[cfg(unix)]
#[test]
fn named_solver_stub_rejects_partial_protocol() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!(
        "cgx2-solver-protocol-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let executable = directory.join("z3");
    fs::write(&executable, b"#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'Z3 version 4.13.0'; exit 0; fi\nif [ \"$1\" != -in ] || [ \"$2\" != -smt2 ]; then exit 4; fi\ncat >/dev/null\nprintf 'unsat\\nsat\\nunsat\\n'\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let input = fixture(include_str!("../examples/valid.json"));
    let result = parsed(
        &input,
        &["--format", "json", "--solver", executable.to_str().unwrap()],
    );
    assert_eq!(result["solver"]["family"], "z3");
    assert_eq!(result["solver"]["arguments"], json!(["-in", "-smt2"]));
    assert_eq!(result["solver"]["identity_valid"], true);
    assert_eq!(
        result["solver"]["executable_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(result["solver"]["response"], "unknown:MalformedResponse");
    assert_eq!(result["solver"]["bounded_result"], "unknown");
    assert_eq!(result["proof"]["kind"], "unknown");
    fs::remove_file(executable).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn solver_timeout_large_and_stalled_output_are_bounded() {
    let input = fixture(include_str!("../examples/valid.json"));
    let timeout = fake_solver("sleep 2");
    let result = parsed(
        &input,
        &[
            "--format",
            "json",
            "--solver",
            timeout.path.to_str().unwrap(),
            "--timeout-ms",
            "50",
        ],
    );
    assert_eq!(result["solver"]["exit"]["kind"], "timeout");
    let large = fake_solver("head -c 200000 /dev/zero");
    let result = parsed(
        &input,
        &[
            "--format",
            "json",
            "--solver",
            large.path.to_str().unwrap(),
            "--timeout-ms",
            "300",
        ],
    );
    assert_eq!(result["solver"]["output_limit_exceeded"], true);
    assert!(result["solver"]["stdout"].as_str().unwrap().len() <= 16 * 1024);
    let stalled = fake_solver("printf 'unsat\\nsat\\nunsat\\n'; sleep 2");
    let result = parsed(
        &input,
        &[
            "--format",
            "json",
            "--solver",
            stalled.path.to_str().unwrap(),
            "--timeout-ms",
            "50",
        ],
    );
    assert_eq!(result["solver"]["exit"]["kind"], "timeout");
    assert_eq!(result["solver"]["trust"], "untrusted_protocol");
    let stderr_stall = fake_solver("printf warning >&2; sleep 2");
    let result = parsed(
        &input,
        &[
            "--format",
            "json",
            "--solver",
            stderr_stall.path.to_str().unwrap(),
            "--timeout-ms",
            "50",
        ],
    );
    assert_eq!(result["solver"]["exit"]["kind"], "timeout");
    assert_eq!(result["solver"]["stderr"], "warning");
}
