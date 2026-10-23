use cgx2_foreign::{ForeignError, parse_declarations};
#[cfg(feature = "in-process")]
use cgx2_python_bridge::{BridgeValue, PythonBridge};

fn valid() -> String {
    r#"{"version":1,"calls":[{"key":"sqrt","module":"math","attribute":["sqrt"],"positional":["f64"],"keywords":{},"returns":"f64","effect":"Foreign","ownership":"copied"}]}"#.into()
}

#[test]
#[cfg(feature = "in-process")]
fn valid_declaration_is_canonical_and_does_not_import() {
    let bridge = PythonBridge::new().unwrap();
    let sys = bridge.import_module("sys").unwrap();
    let modules = bridge.getattr(&sys, "modules").unwrap();
    let get = bridge.getattr(&modules, "get").unwrap();
    let target = "cgx2_parse_probe_module_9";
    let before = bridge
        .call(&get, &[BridgeValue::String(target.into())])
        .unwrap();
    assert!(matches!(
        bridge.to_value(&before).unwrap(),
        BridgeValue::None
    ));

    let input = valid().replace("\"math\"", &format!("\"{target}\""));
    let registry = parse_declarations(&input).unwrap();
    assert_eq!(registry.get("sqrt").unwrap().module(), target);
    assert_eq!(registry.get("sqrt").unwrap().effect(), "Foreign");
    assert_eq!(registry.get("sqrt").unwrap().ownership(), "copied");
    let round_trip = parse_declarations(&registry.to_json()).unwrap();
    assert_eq!(registry.to_json(), round_trip.to_json());

    let after = bridge
        .call(&get, &[BridgeValue::String(target.into())])
        .unwrap();
    assert!(matches!(
        bridge.to_value(&after).unwrap(),
        BridgeValue::None
    ));
}

#[test]
fn malformed_declarations_fail_closed() {
    let base = valid();
    let call = |text: &str| parse_declarations(text).unwrap_err();
    for text in [
        "not json".into(),
        base.replace("\"version\":1", "\"version\":2"),
        base.replace("\"version\":1", "\"version\":true"),
        base.replace("\"calls\"", "\"unknown\""),
        base.replace("\"effect\":\"Foreign\"", "\"effect\":\"Pure\""),
        base.replace(
            "\"ownership\":\"copied\"",
            "\"ownership\":\"python_handle\"",
        ),
        base.replace("\"returns\":\"f64\"", "\"returns\":\"tensor\""),
        base.replace("\"module\":\"math\"", "\"module\":\"math.*\""),
        base.replace("\"attribute\":[\"sqrt\"]", "\"attribute\":[\"*\"]"),
        base.replace("\"attribute\":[\"sqrt\"]", "\"attribute\":[]"),
        base.replace("\"key\":\"sqrt\"", "\"key\":\"*\""),
        base.replace("\"positional\":[\"f64\"]", "\"positional\":[\"nope\"]"),
        base.replace("\"keywords\":{}", "\"keywords\":{\"bad-key\":\"bool\"}"),
        base.replace("\"ownership\":\"copied\"", "\"ownership\":\"borrowed\""),
        base.replace(
            "\"ownership\":\"copied\"",
            "\"ownership\":\"copied\",\"extra\":1",
        ),
        base.replace(
            "\"module\":\"math\",\"attribute\":[\"sqrt\"]",
            "\"module\":\"builtins\",\"attribute\":[\"eval\"]",
        ),
    ] {
        assert!(
            matches!(call(&text), ForeignError::Declaration(_)),
            "{text}"
        );
    }
    // Explicit duplicate entries, independently of JSON object-field ordering.
    let one: serde_json::Value = serde_json::from_str(&base).unwrap();
    let entry = one["calls"][0].clone();
    let duplicate = serde_json::json!({"version":1,"calls":[entry.clone(),entry]}).to_string();
    assert!(matches!(call(&duplicate), ForeignError::Declaration(_)));
    assert!(matches!(
        call(&base.replace("\"version\":1", "\"version\":1,\"version\":1")),
        ForeignError::Declaration(_)
    ));
    assert!(matches!(
        call(&base.replace(
            "\"keywords\":{}",
            "\"keywords\":{\"x\":\"i64\",\"x\":\"bool\"}"
        )),
        ForeignError::Declaration(_)
    ));
    assert!(matches!(
        call(&"x".repeat(65_537)),
        ForeignError::Declaration(_)
    ));
}

#[test]
fn metadata_round_trip_needs_no_python_host() {
    let registry = parse_declarations(&valid()).unwrap();
    assert_eq!(registry.get("sqrt").unwrap().module(), "math");
    let canonical = registry.to_json();
    assert_eq!(parse_declarations(&canonical).unwrap().to_json(), canonical);
}
