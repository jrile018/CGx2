use cgx2_package::load_checked_package;
use cgx2_security::package_v1::{
    audit_checked_package, audit_package, parse_package_policy, AnalysisLimits, PackageReport,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(base: &str, middle: &str, main: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "security-package-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("cgx2.toml"),"schema_version = 2\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\",\"src/middle.cg\",\"src/base.cg\"]\nbinding_lock = \"cgx2-bind.lock.json\"\n").unwrap();
        fs::write(
            root.join("src/base.cg"),
            format!("module demo.base\n{base}"),
        )
        .unwrap();
        fs::write(
            root.join("src/middle.cg"),
            format!("module demo.middle\nimport demo.base\n{middle}"),
        )
        .unwrap();
        fs::write(
            root.join("src/main.cg"),
            format!("module demo.main\nimport demo.middle\n{main}"),
        )
        .unwrap();
        Self(root)
    }
    fn report(&self) -> PackageReport {
        audit_package(
            &self.0,
            &parse_package_policy(POLICY).unwrap(),
            AnalysisLimits::default(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const POLICY: &str = r#"{"schema_version":2,"allowed_effects":["ReadArgs","Foreign","Mutate"],"allowed_capabilities":["ArgsRead","ForeignCall"],"allow_unknown":false}"#;
const SINK:&str="foreign python pub fn send(x: String) -> String effects(Foreign) caps(ForeignCall) = builtins.str runtime \"local:cpython\" ownership copied\n";
const FORWARD:&str="pub fn forward(x: String) -> String effects(Foreign) caps(ForeignCall):\n    demo.base.send(x)\n";
fn chain(argument: &str) -> Fixture {
    Fixture::new(SINK,FORWARD,&format!("fn pick(x: String, y: String) -> String:\n    y\npub fn run() -> String effects(ReadArgs, Foreign) caps(ArgsRead, ForeignCall):\n    demo.middle.forward({argument})\n"))
}

#[test]
fn contract_only_helpers_reach_public_security_graph_without_inventing_findings() {
    use cgx2_effect_index::package_v1::{Identity, Resolution};

    let f = Fixture::new(
        "pub fn helper(x: Bool) -> Bool:\n    x\n",
        "pub fn helper(x: Bool) -> Bool:\n    x\n",
        "# café 🦀\nfn helper(x: Bool) -> Bool:\n    x\npub fn run(x: Bool) -> Bool effects() caps():\n    requires helper(helper(x))\n    ensures helper(result)\n    ensures helper(result)\n    let helper = false\n    x\n",
    );
    let report = f.report();
    assert!(report.findings().is_empty());
    let run = report
        .analysis()
        .entry(&Identity {
            source: "src/main.cg".into(),
            name: "run".into(),
        })
        .unwrap();
    assert_eq!(run.calls.len(), 4);
    assert!(run.effects.is_empty());
    assert!(run.capabilities.is_empty());
    let helper = Identity {
        source: "src/main.cg".into(),
        name: "helper".into(),
    };
    assert!(run
        .calls
        .iter()
        .all(|c| c.resolution == Resolution::Function(helper.clone())));
    let source = fs::read_to_string(f.0.join("src/main.cg")).unwrap();
    let sites: Vec<_> = run
        .calls
        .iter()
        .map(|c| &source[c.location.span.start..c.location.span.end])
        .collect();
    assert_eq!(
        sites,
        [
            "helper(helper(x))",
            "helper(x)",
            "helper(result)",
            "helper(result)"
        ]
    );
    let json = report.to_json().unwrap();
    assert_eq!(json, f.report().to_json().unwrap());
    let json: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["analysis"], "checked_package_security_v1");
    assert_eq!(json["provenance"].as_array().unwrap().len(), 3);
    let sarif = report.to_sarif().unwrap();
    assert_eq!(sarif, f.report().to_sarif().unwrap());
    let sarif: Value = serde_json::from_str(&sarif).unwrap();
    assert_eq!(sarif["version"], "2.1.0");
    assert!(sarif["runs"][0]["results"].as_array().unwrap().is_empty());

    fs::write(f.0.join("src/main.cg"), "module demo.main\n").unwrap();
    assert!(report.to_json().is_err());
    assert!(report.to_sarif().is_err());
}

#[test]
fn unknown_contract_calls_have_distinct_source_located_policy_findings() {
    let f = Fixture::new(
        "pub fn helper() -> Bool:\n    true\n",
        "pub fn helper() -> Bool:\n    true\n",
        "# café 🦀\nfn result() -> Bool:\n    true\npub fn run() -> Bool:\n    ensures result()\n    ensures result()\n    true\n",
    );
    let report = f.report();
    let source = fs::read_to_string(f.0.join("src/main.cg")).unwrap();
    let findings: Vec<_> = report
        .findings()
        .iter()
        .filter(|f| {
            f.rule_id == "CGSEC002"
                && &source[f.location.span.start..f.location.span.end] == "result()"
        })
        .collect();
    assert_eq!(findings.len(), 2);
    assert_ne!(findings[0].location.span, findings[1].location.span);
    assert!(!report
        .findings()
        .iter()
        .any(|f| matches!(f.rule_id, "CGSEC001" | "CGSEC003" | "CGSEC005")));
    let sarif: Value = serde_json::from_str(&report.to_sarif().unwrap()).unwrap();
    assert_eq!(
        sarif["runs"][0]["results"].as_array().unwrap().len(),
        report.findings().len()
    );
}

#[test]
fn transitive_taint_reaches_checked_foreign_sink_with_cross_file_origin_and_declaration() {
    let f = chain("arg(0)");
    let report = f.report();
    let finding = report
        .findings()
        .iter()
        .find(|f| f.rule_id == "CGSEC003")
        .unwrap();
    assert_eq!(finding.location.source, PathBuf::from("src/middle.cg"));
    assert_eq!(finding.related[0].source, PathBuf::from("src/main.cg"));
    assert_eq!(finding.related[1].source, PathBuf::from("src/base.cg"));
    let main = fs::read_to_string(f.0.join("src/main.cg")).unwrap();
    assert_eq!(
        &main[finding.related[0].span.start..finding.related[0].span.end],
        "arg(0)"
    );
    assert!(!report
        .findings()
        .iter()
        .any(|f| matches!(f.rule_id, "CGSEC001" | "CGSEC002" | "CGSEC005")));
}
#[test]
fn clean_literal_is_not_tainted_but_unknown_public_parameters_remain_advisory() {
    let f = chain("\"safe\"");
    let report = f.report();
    assert!(!report.findings().iter().any(|f| f.rule_id == "CGSEC003"));
    assert!(report.findings().iter().any(|f| f.rule_id == "CGSEC004"));
}
#[test]
fn source_in_imported_body_forwarded_to_local_sink() {
    let f=Fixture::new("pub fn input() -> String effects(ReadArgs) caps(ArgsRead):\n    arg(0)\n", "pub fn forward() -> String effects(ReadArgs) caps(ArgsRead):\n    demo.base.input()\n", &format!("{SINK}pub fn run() -> String effects(ReadArgs, Foreign) caps(ArgsRead, ForeignCall):\n    send(demo.middle.forward())\n"));
    let report = f.report();
    let finding = report
        .findings()
        .iter()
        .find(|f| f.rule_id == "CGSEC003")
        .unwrap();
    assert_eq!(finding.location.source, PathBuf::from("src/main.cg"));
    assert_eq!(finding.related[0].source, PathBuf::from("src/base.cg"));
}
#[test]
fn inactive_branches_and_branch_shadowing_are_conservative_and_scoped() {
    for body in [
        "    let mut x = \"safe\"\n    if false:\n        x = arg(0)\n    else:\n        0\n    return demo.middle.forward(x)\n",
        "    let x = arg(0)\n    if true:\n        let x = \"safe\"\n    else:\n        0\n    return demo.middle.forward(x)\n",
    ] {
        let f=Fixture::new(SINK,FORWARD,&format!("pub fn run() -> String effects(ReadArgs, Foreign, Mutate) caps(ArgsRead, ForeignCall):\n{body}"));
        assert!(f.report().findings().iter().any(|f|f.rule_id=="CGSEC003"));
    }
    let f=Fixture::new(SINK,FORWARD,"pub fn run() -> String effects(ReadArgs, Foreign) caps(ArgsRead, ForeignCall):\n    let x = \"safe\"\n    if true:\n        let x = arg(0)\n    else:\n        0\n    return demo.middle.forward(x)\n");
    assert!(!f
        .report()
        .findings()
        .iter()
        .any(|f| f.rule_id == "CGSEC003"));
}
#[test]
fn loop_carried_values_unknown_even_when_iteration_count_is_zero() {
    let f=Fixture::new(SINK,FORWARD,"pub fn run() -> String effects(ReadArgs, Foreign, Mutate) caps(ArgsRead, ForeignCall):\n    let mut x = \"safe\"\n    for i in range(0, 0):\n        x = \"safe\"\n    return demo.middle.forward(x)\n");
    let report = f.report();
    assert!(report
        .findings()
        .iter()
        .any(|f| f.message.contains("loop-carried")));
    assert!(report
        .findings()
        .iter()
        .any(|f| f.message.contains("foreign argument taint is unknown")));
}
#[test]
fn recursive_calls_and_global_step_depth_budgets_are_explicit_unknowns() {
    let f = chain("arg(0)");
    let policy = parse_package_policy(POLICY).unwrap();
    let report = audit_package(
        &f.0,
        &policy,
        AnalysisLimits {
            max_steps: 1,
            max_call_depth: 16,
        },
    )
    .unwrap();
    assert!(report
        .findings()
        .iter()
        .any(|f| f.message.contains("budget exhausted")));
    let report = audit_package(
        &f.0,
        &policy,
        AnalysisLimits {
            max_steps: 16_384,
            max_call_depth: 1,
        },
    )
    .unwrap();
    assert!(report
        .findings()
        .iter()
        .any(|f| f.message.contains("over-depth")));
    assert!(audit_package(
        &f.0,
        &policy,
        AnalysisLimits {
            max_steps: 0,
            max_call_depth: 16
        }
    )
    .is_err());
    let f=Fixture::new("pub fn rec(x: String) -> String:\n    rec(x)\n", "pub fn forward(x: String) -> String:\n    demo.base.rec(x)\n", "pub fn run() -> String effects(ReadArgs) caps(ArgsRead):\n    demo.middle.forward(arg(0))\n");
    assert!(f
        .report()
        .findings()
        .iter()
        .any(|f| f.message.contains("recursive")));
}
#[test]
fn package_policy_checks_hidden_effects_and_capabilities_at_call_and_declaration() {
    let f = chain("arg(0)");
    let policy = parse_package_policy(
        r#"{"schema_version":2,"allowed_effects":[],"allowed_capabilities":[]}"#,
    )
    .unwrap();
    let report = audit_package(&f.0, &policy, AnalysisLimits::default()).unwrap();
    assert!(report
        .findings()
        .iter()
        .any(|f| f.rule_id == "CGSEC001" && !f.related.is_empty()));
    assert!(report.findings().iter().any(|f| f.rule_id == "CGSEC005"
        && f.message.contains("ForeignCall")
        && !f.related.is_empty()));
}
#[test]
fn stable_json_sarif_utf8_columns_and_relative_paths() {
    let f = chain("\"é😀\"");
    let report = f.report();
    assert_eq!(report.to_json().unwrap(), report.to_json().unwrap());
    assert_eq!(report.to_sarif().unwrap(), report.to_sarif().unwrap());
    assert!(!report
        .to_sarif()
        .unwrap()
        .contains(&f.0.to_string_lossy().to_string()));
    let sarif: Value = serde_json::from_str(&report.to_sarif().unwrap()).unwrap();
    for result in sarif["runs"][0]["results"].as_array().unwrap() {
        let physical = &result["locations"][0]["physicalLocation"];
        assert!(physical["artifactLocation"]["uri"]
            .as_str()
            .unwrap()
            .starts_with("src/"));
        assert!(physical["region"]["startLine"].as_u64().unwrap() > 0);
    }
    // A taint origin after non-ASCII text uses Unicode columns rather than byte offsets.
    let f = chain("pick(\"é😀\", arg(0))");
    let report = f.report();
    let sarif: Value = serde_json::from_str(&report.to_sarif().unwrap()).unwrap();
    let result = sarif["runs"][0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["ruleId"] == "CGSEC003")
        .unwrap();
    let text = fs::read_to_string(f.0.join("src/main.cg")).unwrap();
    let prefix = text.lines().last().unwrap().split("arg(0)").next().unwrap();
    assert_eq!(
        result["relatedLocations"][0]["physicalLocation"]["region"]["startColumn"],
        prefix.chars().count() + 1
    );
}
#[test]
fn report_and_input_reject_mutated_provenance() {
    let f = chain("arg(0)");
    let checked = load_checked_package(&f.0).unwrap();
    let report = f.report();
    fs::write(
        f.0.join("src/base.cg"),
        format!("module demo.base\n{SINK}# drift\n"),
    )
    .unwrap();
    assert!(audit_checked_package(
        &checked,
        &parse_package_policy(POLICY).unwrap(),
        AnalysisLimits::default()
    )
    .is_err());
    assert!(report.to_json().is_err());
    assert!(report.to_sarif().is_err());
    assert!(report.to_text().is_err());
}
#[test]
fn schema2_policy_and_command_are_additive() {
    for source in [
        r#"{"schema_version":1,"allowed_effects":[],"allowed_capabilities":[]}"#,
        r#"{"schema_version":2,"allowed_effects":[]}"#,
        r#"{"schema_version":2,"allowed_effects":[],"allowed_capabilities":["ArgsRead","ArgsRead"]}"#,
        r#"{"schema_version":2,"allowed_effects":[],"allowed_capabilities":[],"extra":true}"#,
    ] {
        assert!(parse_package_policy(source).is_err());
    }
    let f = chain("arg(0)");
    fs::write(f.0.join("policy.json"), POLICY).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cgx2-security"))
        .args(["audit", "--package"])
        .arg(&f.0)
        .arg("--policy")
        .arg(f.0.join("policy.json"))
        .args(["--format", "sarif"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["version"], "2.1.0");
}

#[test]
fn pure_imported_wrappers_are_clean_and_cli_distinguishes_invalid_packages() {
    let f = Fixture::new(
        "pub fn value() -> i64 effects() caps():\n    7\n",
        "pub fn forward() -> i64 effects() caps():\n    demo.base.value()\n",
        "pub fn run() -> i64 effects() caps():\n    demo.middle.forward()\n",
    );
    let policy = r#"{"schema_version":2,"allowed_effects":[],"allowed_capabilities":[]}"#;
    fs::write(f.0.join("policy.json"), policy).unwrap();
    assert!(audit_package(
        &f.0,
        &parse_package_policy(policy).unwrap(),
        AnalysisLimits::default()
    )
    .unwrap()
    .findings()
    .is_empty());
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_cgx2-security"))
            .args(["audit", "--package"])
            .arg(&f.0)
            .arg("--policy")
            .arg(f.0.join("policy.json"))
            .args(["--format", "json"])
            .output()
            .unwrap()
    };
    assert_eq!(run().status.code(), Some(0));
    fs::write(
        f.0.join("src/base.cg"),
        "module demo.base\npub fn value() -> i64:\n    missing()\n",
    )
    .unwrap();
    assert_eq!(run().status.code(), Some(2));
}
#[test]
fn shipped_source_example_is_checked_and_advisory_only() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/package-audit");
    let policy =
        parse_package_policy(&fs::read_to_string(root.join("policy.json")).unwrap()).unwrap();
    let report = audit_package(&root, &policy, AnalysisLimits::default()).unwrap();
    assert!(report.findings().iter().any(|f| f.rule_id == "CGSEC003"));
    assert!(report
        .findings()
        .iter()
        .any(|f| f.message.contains("foreign return taint is unknown")));
}
