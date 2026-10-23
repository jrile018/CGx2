use cgx2_security::{audit, parse_policy};

fn policy(effects: &[&str], unknown: bool) -> cgx2_security::Policy {
    let allowed = effects
        .iter()
        .map(|effect| format!("\"{effect}\""))
        .collect::<Vec<_>>()
        .join(",");
    parse_policy(&format!(
        "{{\"schema_version\":1,\"allowed_effects\":[{allowed}],\"allow_unknown\":{unknown}}}"
    ))
    .unwrap()
}

#[test]
fn direct_transitive_allowed_and_imported_effects() {
    let source = "fn leaf() -> i64:\n    arg_count()\nfn root() -> i64:\n    leaf()\n";
    let report = audit(source, &policy(&[], false)).unwrap();
    assert!(report
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC001" && f.span.start == source.find("leaf()\n").unwrap()));
    assert!(report
        .findings
        .iter()
        .any(|f| f.message.contains("ReadArgs")));
    assert!(audit(source, &policy(&["ReadArgs"], false))
        .unwrap()
        .findings
        .is_empty());

    let imported = "import app.math\nfn root() -> i64:\n    app.math.double(1)\n";
    assert!(audit(imported, &policy(&[], false))
        .unwrap()
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC002"));
    assert!(audit(imported, &policy(&[], true))
        .unwrap()
        .findings
        .is_empty());

    let shadowed_builtin = "import memory\nfn root() -> i64:\n    memory.copy_to_owned(1)\n";
    let report = audit(shadowed_builtin, &policy(&["ManagedAlloc"], false)).unwrap();
    assert!(report.findings.iter().any(|f| f.rule_id == "CGSEC002"));
    assert!(!report.findings.iter().any(|f| f.rule_id == "CGSEC001"));
}

#[test]
fn conversion_effects_and_policy_validation() {
    let source = "let r = memory.region(mode: manual, limit: 64)\nlet x = memory.owned_bytes(4)\nlet y = memory.promote_owned(r, x)\nlet z = memory.copy_to_owned(y)\n";
    let report = audit(source, &policy(&["ManagedAlloc"], true)).unwrap();
    assert!(report.findings.iter().any(|f| f.message.contains("Mutate")
        && f.span.start == source.find("memory.promote_owned").unwrap()));
    assert!(audit(source, &policy(&["ManagedAlloc", "Mutate"], true))
        .unwrap()
        .findings
        .is_empty());
    for invalid in [
        "{}",
        "{\"schema_version\":2,\"allowed_effects\":[]}",
        "{\"schema_version\":1,\"allowed_effects\":[\"Bogus\"]}",
        "{\"schema_version\":1,\"allowed_effects\":[],\"extra\":true}",
    ] {
        assert!(parse_policy(invalid).is_err(), "{invalid}");
    }
    assert!(audit("fn bad(:\n", &policy(&[], false)).is_err());
}

#[test]
fn taint_binding_local_call_clean_literal_and_unknown_alias() {
    let direct = include_str!("fixtures/tainted_binding.cg");
    let report = audit(direct, &policy(&["ReadArgs", "Foreign"], true)).unwrap();
    let taint = report
        .findings
        .iter()
        .find(|f| f.rule_id == "CGSEC003")
        .unwrap();
    assert_eq!(taint.related.unwrap().start, direct.find("arg(0)").unwrap());
    assert_eq!(taint.span.start, direct.find("python.call").unwrap());

    let local = include_str!("fixtures/local_forward.cg");
    assert!(audit(local, &policy(&["ReadArgs", "Foreign"], true))
        .unwrap()
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC003"));
    assert!(audit(
        include_str!("fixtures/clean_literal.cg"),
        &policy(&["Foreign"], true)
    )
    .unwrap()
    .findings
    .is_empty());
    let alias = include_str!("fixtures/unknown_alias.cg");
    assert!(audit(alias, &policy(&["ReadArgs", "Foreign"], true))
        .unwrap()
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC004"));
    let shadowed_source =
        "fn arg(x: i64) -> i64:\n    x\nfn run() -> i64:\n    python.call(\"eval\", arg(0))\n";
    assert!(!audit(shadowed_source, &policy(&["Foreign"], true))
        .unwrap()
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC003"));

    let standalone_unknown =
        "fn relay(x: i64) -> i64:\n    python.call(\"exec\", x)\nfn run() -> i64:\n    relay(7)\n";
    assert!(audit(standalone_unknown, &policy(&["Foreign"], true))
        .unwrap()
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC004"));
}

#[test]
fn deterministic_json_and_sarif_unicode_locations() {
    let source = "fn café() -> i64:\n    python.call(\"exec\", arg(0))\n";
    let report = audit(source, &policy(&["ReadArgs", "Foreign"], true)).unwrap();
    assert_eq!(
        report.to_json(),
        audit(source, &policy(&["ReadArgs", "Foreign"], true))
            .unwrap()
            .to_json()
    );
    let value: serde_json::Value =
        serde_json::from_str(&report.to_sarif("example.cg", source)).unwrap();
    assert_eq!(value["version"], "2.1.0");
    assert_eq!(value["runs"][0]["tool"]["driver"]["name"], "cgx2-security");
    assert_eq!(value["runs"][0]["columnKind"], "unicodeCodePoints");
    let results = value["runs"][0]["results"].as_array().unwrap();
    let taint = results.iter().find(|r| r["ruleId"] == "CGSEC003").unwrap();
    assert_eq!(
        taint["locations"][0]["physicalLocation"]["region"]["startLine"],
        2
    );
    assert_eq!(
        taint["locations"][0]["physicalLocation"]["region"]["startColumn"],
        5
    );
    assert!(taint["relatedLocations"].is_array());
    assert!(!report.to_sarif("example.cg", source).contains("fn café"));
    assert_eq!(
        value["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
            ["uri"],
        "example.cg"
    );
    assert!(report
        .to_sarif("my file.cg", source)
        .contains("my%20file.cg"));

    let same_line = "let café = python.call(\"exec\", 3)\n";
    let value: serde_json::Value = serde_json::from_str(
        &audit(same_line, &policy(&[], true))
            .unwrap()
            .to_sarif("unicode.cg", same_line),
    )
    .unwrap();
    assert_eq!(
        value["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]["startColumn"],
        12
    );
}

#[test]
fn policy_and_source_size_are_bounded() {
    assert!(parse_policy(&" ".repeat(cgx2_security::MAX_POLICY_BYTES + 1)).is_err());
    assert!(audit(
        &" ".repeat(cgx2_effect_index::MAX_SOURCE_BYTES + 1),
        &policy(&[], false)
    )
    .is_err());
}

#[test]
fn conditional_taint_joins_assignments_returns_and_keeps_locals_separate() {
    for branch in ["true", "false"] {
        let source = format!("fn relay() -> String:\n    let mut value = \"clean\"\n    if {branch}:\n        value = arg(0)\n        let value = \"shadow\"\n    else:\n        let value = \"local\"\n    return value\nfn main() -> String:\n    python.call(\"eval\", relay())\n");
        let report = audit(&source, &policy(&["ReadArgs", "Foreign", "Mutate"], true)).unwrap();
        assert!(
            report.findings.iter().any(|f| f.rule_id == "CGSEC003"),
            "{report:?}"
        );
    }
    let source = "fn relay() -> String:\n    if false:\n        return arg(0)\n    else:\n        return \"clean\"\nfn main() -> String:\n    python.call(\"eval\", relay())\n";
    assert!(audit(source, &policy(&["ReadArgs", "Foreign"], true))
        .unwrap()
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC003"));
    let mixed = "fn relay() -> String:\n    if false:\n        return arg(0)\n    else:\n        \"ignored\"\n    return \"clean\"\nfn main() -> String:\n    python.call(\"eval\", relay())\n";
    assert!(audit(mixed, &policy(&["ReadArgs", "Foreign"], true))
        .unwrap()
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC003"));
    let source = "fn relay() -> String:\n    let value = \"clean\"\n    if false:\n        let value = arg(0)\n    else:\n        let value = \"local\"\n    value\nfn main() -> String:\n    python.call(\"eval\", relay())\n";
    assert!(!audit(source, &policy(&["ReadArgs", "Foreign"], true))
        .unwrap()
        .findings
        .iter()
        .any(|f| f.rule_id == "CGSEC003"));
}
