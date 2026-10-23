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
fn useful_scalar_profile_source_core_and_smt_parity() {
    for (text, domains, expected) in [
        (
            include_str!("../examples/scalar-v2/valid.cg"),
            vec!["--domain", "x:0:3"],
            [0, 1, 0, 0],
        ),
        (
            include_str!("../examples/scalar-v2/false.cg"),
            vec!["--domain", "x:0:3"],
            [0, 4, 3, 0],
        ),
        (
            include_str!("../examples/scalar-v2/mul-overflow.cg"),
            vec!["--domain", "x:9223372036854775807:9223372036854775807"],
            [0, 1, 0, 1],
        ),
        (
            include_str!("../examples/scalar-v2/div-zero.cg"),
            vec!["--domain", "x:0:1"],
            [0, 2, 0, 2],
        ),
        (
            include_str!("../examples/scalar-v2/div-min.cg"),
            vec![
                "--domain",
                "x:-9223372036854775808:-9223372036854775808",
                "--domain",
                "y:-1:-1",
            ],
            [0, 1, 0, 1],
        ),
        (
            include_str!("../examples/scalar-v2/div-truncate.cg"),
            vec!["--domain", "x:-7:-7"],
            [0, 1, 0, 0],
        ),
        (
            include_str!("../examples/scalar-v2/inactive.cg"),
            vec!["--domain", "x:-1:1"],
            [0, 3, 0, 0],
        ),
        (
            include_str!("../examples/scalar-v2/shadow.cg"),
            vec!["--domain", "x:-2:2"],
            [0, 1, 0, 0],
        ),
    ] {
        let result = Source::new(text).run(&domains);
        assert_eq!(result["scalar_profile"]["version"], 2);
        assert_eq!(
            result["bounded_solver_result"]["source_parity"], true,
            "{result}"
        );
        for (index, name) in [
            "requires_defined",
            "requires_satisfiable",
            "ensures_false",
            "ensures_overflow",
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(
                result["bounded_solver_result"]["obligations"][name], expected[index],
                "{name}: {result}"
            );
        }
        assert_ne!(
            result["proof"]["kind"], "proved",
            "finite audit never grants global proof"
        );
    }
}

#[test]
fn accumulator_source_is_explicitly_blocked_by_shared_effect_contract() {
    let text = include_str!("../examples/scalar-v2/accumulator.cg");
    for (bound, code) in [
        (None, "CGV001"),
        (Some("2"), "CGV001"),
        (Some("3"), "CGT012"),
    ] {
        let mut args = vec![
            "--domain",
            "x:0:2",
            "--domain",
            "i:7:7",
            "--solver",
            "/definitely/missing/z3",
        ];
        if let Some(bound) = bound {
            args.extend(["--max-expansion", bound]);
        }
        let result = Source::new(text).run(&args);
        assert_eq!(result["verdict"]["diagnostic"]["code"], code, "{result}");
        assert_eq!(result["bounded_solver_result"]["kind"], "unrun");
        assert!(
            result.get("solver").is_none(),
            "solver must not start for rejected source"
        );
    }
}

#[test]
fn inactive_accumulators_share_the_expansion_budget_before_solver_startup() {
    let text = "pub fn f(flag: Bool) -> i64 effects() caps():\n    requires true\n    ensures true\n    if true:\n        let mut a = 0\n        for i in range(0, 3):\n            a += i\n        return a\n    else:\n        let mut b = 0\n        for j in range(0, 3):\n            b += j\n        return b\n";
    let result =
        Source::new(text).run(&["--max-expansion", "5", "--solver", "/definitely/missing/z3"]);
    assert_eq!(result["verdict"]["diagnostic"]["code"], "CGV001");
    assert!(result["verdict"]["diagnostic"]["message"]
        .as_str()
        .unwrap()
        .contains("expansion limit"));
    assert_eq!(result["bounded_solver_result"]["kind"], "unrun");
    assert!(result.get("solver").is_none());
}

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

#[test]
fn conditional_contract_reports_a_replayable_false_branch() {
    let source = Source::new("pub fn choose(flag: Bool) -> i64 effects() caps():\n    requires true\n    ensures result == 1\n    if flag:\n        return 1\n    else:\n        return 2\n");
    let result = source.run(&["--function", "choose"]);
    assert_eq!(result["verdict"]["kind"], "disproved");
    assert_eq!(result["verdict"]["counterexample"]["flag"], false);
    assert_eq!(result["verdict"]["failure"], "EnsuresFalse");
    assert_eq!(result["bounded_solver_result"]["source_parity"], true);
    assert_eq!(
        result["bounded_solver_result"]["obligations"]["ensures_false"],
        1
    );
}

#[test]
fn nested_trees_and_both_result_sorts_have_independent_encoding_parity() {
    for text in [
        include_str!("../examples/conditional.cg"),
        include_str!("../examples/conditional-bool.cg"),
    ] {
        let result = Source::new(text).run(&["--domain", "x:-1:1"]);
        assert_eq!(result["verdict"]["kind"], "proved", "{result}");
        assert_eq!(result["verdict"]["checked_cases"], 6);
        assert_eq!(result["bounded_solver_result"]["source_parity"], true);
        assert_eq!(result["proof"]["kind"], "unknown");
        assert!(result["smtlib"].as_str().unwrap().contains("(ite "));
    }
}

#[test]
fn only_selected_branch_and_guard_overflow_are_violations() {
    for text in [
        include_str!("../examples/conditional-overflow.cg"),
        include_str!("../examples/conditional-guard-overflow.cg"),
    ] {
        let result =
            Source::new(text).run(&["--domain", "x:9223372036854775807:9223372036854775807"]);
        assert_eq!(result["verdict"]["failure"], "Overflow", "{result}");
        assert_eq!(result["bounded_solver_result"]["source_parity"], true);
    }
    let text = include_str!("../examples/conditional-overflow.cg").replace("if flag:", "if true:");
    let result = Source::new(&text).run(&["--domain", "x:9223372036854775807:9223372036854775807"]);
    assert_eq!(result["verdict"]["kind"], "proved", "{result}");
    assert_eq!(
        result["bounded_solver_result"]["obligations"]["ensures_overflow"],
        0
    );
    assert_eq!(result["proof"]["kind"], "unknown");
    let result =
        Source::new(&text.replace("requires true", "requires false")).run(&["--domain", "x:0:0"]);
    assert_eq!(result["verdict"]["kind"], "unknown");
    assert_eq!(result["proof"]["kind"], "unknown");
}

fn conditional_source(body: &str) -> String {
    format!("# λ Unicode prefix\npub fn f(flag: Bool, x: i64) -> i64 effects() caps():\n    requires true\n    ensures true\n{body}")
}

fn reject_conditional(text: &str) -> Value {
    let result = Source::new(text).run(&["--domain", "x:0:1"]);
    assert_eq!(result["verdict"]["kind"], "unknown", "{result}");
    assert_eq!(
        result["verdict"]["diagnostic"]["code"], "CGV001",
        "{result}"
    );
    assert_eq!(result["bounded_solver_result"]["kind"], "unrun");
    let span = &result["verdict"]["diagnostic"]["span"];
    let start = span["start"].as_u64().unwrap() as usize;
    let end = span["end"].as_u64().unwrap() as usize;
    assert!(start > "# λ Unicode prefix\n".len());
    assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
    assert!(start < end && end <= text.len());
    result
}

#[test]
fn out_of_profile_branches_reject_before_solver_even_when_inactive() {
    for rejected in [
        "        true\n",
        "        return\n",
        "        return true\n",
        "        let mut y = 1\n        y = 2\n        return y\n",
        "        return f(flag, x)\n",
        "        return python.abs(x)\n",
        "        return R(value: x)\n",
        "        return memory.owned_bytes(x)\n",
        "        for y in range(0, 2):\n            x\n",
        "        return 1\n        return 2\n",
    ] {
        reject_conditional(&conditional_source(&format!(
            "    if true:\n        return 1\n    else:\n{rejected}"
        )));
    }
    reject_conditional(&conditional_source(
        "    if x:\n        return 1\n    else:\n        return 2\n",
    ));
}

#[test]
fn incomplete_paths_and_source_logical_syntax_remain_parser_rejections() {
    for body in [
        "    if flag:\n        return 1\n",
        "    if flag:\n        return 1\n    else:\n",
        "    if flag and true:\n        return 1\n    else:\n        return 2\n",
    ] {
        let result = Source::new(&conditional_source(body)).run(&["--domain", "x:0:1"]);
        assert_eq!(result["verdict"]["kind"], "unknown");
        assert_eq!(result["bounded_solver_result"]["kind"], "unrun");
        assert!(result["verdict"]["diagnostic"]["code"]
            .as_str()
            .unwrap()
            .starts_with("CGS"));
    }
}

#[cfg(unix)]
#[test]
fn conditional_sat_transport_replays_selected_overflow_against_source() {
    use std::os::unix::fs::PermissionsExt;
    let directory = std::env::temp_dir().join(format!(
        "cgx2-conditional-solver-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let solver = directory.join("z3");
    for (flag, expected) in [
        ("true", "sat_candidate_unvalidated"),
        ("false", "validated_counterexample"),
    ] {
        // This fixture emulator tests SAT transport/replay only.
        // It has no trusted digest and must never supply proof.
        fs::write(&solver, format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Z3 version 4.12.2'; exit 0; fi\ninput=$(cat)\ncase \"$input\" in\n  *get-value*) printf 'sat\\n((v_666c6167 {flag}) (v_78 9223372036854775807))\\n' ;;\n  *v_666c6167*) printf 'unsat\\nsat\\nunsat\\nsat\\n' ;;\n  *'() Bool false)'*) printf 'unsat\\nsat\\nsat\\nunsat\\n' ;;\n  *'(+ '*) printf 'unsat\\nsat\\nunsat\\nsat\\n' ;;\n  *) printf 'unsat\\nsat\\nunsat\\nunsat\\n' ;;\nesac\n")).unwrap();
        fs::set_permissions(&solver, fs::Permissions::from_mode(0o700)).unwrap();
        let text = include_str!("../examples/conditional-overflow.cg")
            .replace("ensures result == x", "ensures true");
        let result =
            Source::new(&text).run(&["--domain", "x:0:0", "--solver", solver.to_str().unwrap()]);
        assert_eq!(result["solver"]["response"], expected, "{result}");
        assert_ne!(result["proof"]["kind"], "proved");
        if flag == "false" {
            assert_eq!(result["proof"]["kind"], "disproved");
            assert_eq!(
                result["proof"]["meaning"],
                "solver_witness_validated_against_source"
            );
            assert_eq!(result["proof"]["counterexample"]["flag"], false);
            assert_eq!(result["proof"]["counterexample"]["x"], i64::MAX);
        }
    }
    fs::remove_dir_all(directory).unwrap();
}

fn chain(depth: usize) -> String {
    let mut text = String::new();
    for level in 0..depth {
        text.push_str(&format!("{}if flag:\n", "    ".repeat(level + 1)));
    }
    text.push_str(&format!("{}return 1\n", "    ".repeat(depth + 1)));
    for level in (0..depth).rev() {
        text.push_str(&format!(
            "{}else:\n{}return 2\n",
            "    ".repeat(level + 1),
            "    ".repeat(level + 2)
        ));
    }
    text
}

#[test]
fn conditional_depth_and_node_caps_cover_inactive_subtrees() {
    let accepted = Source::new(&conditional_source(&chain(32))).run(&["--domain", "x:0:0"]);
    assert_eq!(accepted["verdict"]["kind"], "proved", "{accepted}");
    assert_eq!(accepted["bounded_solver_result"]["source_parity"], true);
    let rejected = reject_conditional(&conditional_source(&chain(33)));
    assert!(rejected["verdict"]["diagnostic"]["message"]
        .as_str()
        .unwrap()
        .contains("depth limit"));
    fn tree(depth: usize, indent: usize) -> String {
        let pad = "    ".repeat(indent);
        if depth == 0 {
            return format!("{pad}return 1\n");
        }
        format!(
            "{pad}if true:\n{}{pad}else:\n{}",
            tree(depth - 1, indent + 1),
            tree(depth - 1, indent + 1)
        )
    }
    let accepted = Source::new(&conditional_source(&tree(8, 1))).run(&["--domain", "x:0:0"]);
    assert_eq!(accepted["verdict"]["kind"], "proved");
    assert_eq!(accepted["bounded_solver_result"]["source_parity"], true);
    let rejected = reject_conditional(&conditional_source(&tree(8, 1).replacen(
        "return 1",
        "return 1 + 0",
        1,
    )));
    assert!(rejected["verdict"]["diagnostic"]["message"]
        .as_str()
        .unwrap()
        .contains("node limit"));
}

#[test]
fn s2a_variants_are_unsupported_and_never_proved() {
    let source = Source::new("pub variant Reply:\n    Empty\npub fn check(x: i64) -> i64 effects():\n    ensures result == x\n    x\n");
    let result = source.run(&[
        "--function",
        "check",
        "--domain",
        "x:0:2",
        "--solver",
        "/definitely/missing/z3",
    ]);
    assert_eq!(result["verdict"]["kind"], "unknown");
    assert_eq!(result["proof"]["state"], "unsupported");
    assert_eq!(result["bounded_solver_result"]["kind"], "unrun");
    assert!(result["verdict"]["diagnostic"]["message"]
        .as_str()
        .unwrap()
        .contains("S3-V"));
    assert!(result.get("solver").is_none());
}
