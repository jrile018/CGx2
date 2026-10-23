#![cfg(feature = "in-process")]

use cgx2_foreign::{ForeignError, Registry, parse_declarations};
use cgx2_python_bridge::{BridgeError, BridgeValue, PythonBridge};
use std::collections::BTreeMap;

fn registry(calls: serde_json::Value) -> Registry {
    parse_declarations(&serde_json::json!({"version":1,"calls":calls}).to_string()).unwrap()
}

fn declaration(
    key: &str,
    module: &str,
    attribute: &[&str],
    positional: &[&str],
    keywords: serde_json::Value,
    returns: &str,
    ownership: &str,
) -> serde_json::Value {
    serde_json::json!({
        "key":key,"module":module,"attribute":attribute,"positional":positional,
        "keywords":keywords,"returns":returns,"effect":"Foreign","ownership":ownership
    })
}

#[test]
fn scalar_keyword_and_structured_stdlib_calls() {
    let bridge = PythonBridge::new().unwrap();
    let registry = registry(serde_json::json!([
        declaration(
            "sqrt",
            "math",
            &["sqrt"],
            &["f64"],
            serde_json::json!({}),
            "f64",
            "copied"
        ),
        declaration(
            "dumps",
            "json",
            &["dumps"],
            &["dict"],
            serde_json::json!({"sort_keys":"bool"}),
            "string",
            "copied"
        ),
        declaration(
            "loads",
            "json",
            &["loads"],
            &["string"],
            serde_json::json!({}),
            "dict",
            "copied"
        )
    ]));
    assert!(matches!(
        registry
            .call(&bridge, "sqrt", &[BridgeValue::F64(9.0)], &[])
            .unwrap(),
        BridgeValue::F64(3.0)
    ));
    let mut input = BTreeMap::new();
    input.insert("b".into(), BridgeValue::List(vec![BridgeValue::I64(2)]));
    input.insert("a".into(), BridgeValue::Bool(true));
    let text = registry
        .call(
            &bridge,
            "dumps",
            &[BridgeValue::Dict(input)],
            &[("sort_keys", BridgeValue::Bool(true))],
        )
        .unwrap();
    let BridgeValue::String(text) = text else {
        panic!("expected copied string")
    };
    assert_eq!(text, "{\"a\": true, \"b\": [2]}");
    let decoded = registry
        .call(&bridge, "loads", &[BridgeValue::String(text)], &[])
        .unwrap();
    assert!(matches!(decoded, BridgeValue::Dict(values) if values.len() == 2));
}

#[test]
fn blocked_and_invalid_arguments_do_not_import_or_call() {
    let bridge = PythonBridge::new().unwrap();
    let sys = bridge.import_module("sys").unwrap();
    let modules = bridge.getattr(&sys, "modules").unwrap();
    let get = bridge.getattr(&modules, "get").unwrap();
    let target = "cgx2_foreign_never_imported_9";
    let registry = registry(serde_json::json!([declaration(
        "probe",
        target,
        &["call"],
        &["string"],
        serde_json::json!({"flag":"bool"}),
        "none",
        "copied"
    )]));
    let check_absent = || {
        let value = bridge
            .call(&get, &[BridgeValue::String(target.into())])
            .unwrap();
        assert!(matches!(
            bridge.to_value(&value).unwrap(),
            BridgeValue::None
        ));
    };
    check_absent();
    assert_eq!(
        registry
            .call(&bridge, "unknown", &[], &[])
            .unwrap_err()
            .category(),
        "blocked_call"
    );
    assert_eq!(
        registry
            .call(&bridge, "probe", &[], &[])
            .unwrap_err()
            .category(),
        "argument"
    );
    assert_eq!(
        registry
            .call(&bridge, "probe", &[BridgeValue::I64(1)], &[])
            .unwrap_err()
            .category(),
        "argument"
    );
    assert_eq!(
        registry
            .call(
                &bridge,
                "probe",
                &[BridgeValue::String("x".into())],
                &[("other", BridgeValue::Bool(true))]
            )
            .unwrap_err()
            .category(),
        "argument"
    );
    assert_eq!(
        registry
            .call(
                &bridge,
                "probe",
                &[BridgeValue::String("x".into())],
                &[
                    ("flag", BridgeValue::Bool(true)),
                    ("flag", BridgeValue::Bool(false))
                ]
            )
            .unwrap_err()
            .category(),
        "argument"
    );
    assert_eq!(
        registry
            .call(
                &bridge,
                "probe",
                &[BridgeValue::String("x".repeat(1_048_577))],
                &[]
            )
            .unwrap_err()
            .category(),
        "argument"
    );
    check_absent();
}

#[test]
fn lookup_exception_and_conversion_are_distinct() {
    let bridge = PythonBridge::new().unwrap();
    let registry = registry(serde_json::json!([
        declaration(
            "missing_module",
            "cgx2_missing_module_9",
            &["f"],
            &[],
            serde_json::json!({}),
            "none",
            "copied"
        ),
        declaration(
            "missing_attribute",
            "math",
            &["cgx2_missing_attribute_9"],
            &[],
            serde_json::json!({}),
            "none",
            "copied"
        ),
        declaration(
            "sqrt",
            "math",
            &["sqrt"],
            &["f64"],
            serde_json::json!({}),
            "f64",
            "copied"
        ),
        declaration(
            "bad_json",
            "json",
            &["loads"],
            &["string"],
            serde_json::json!({}),
            "dict",
            "copied"
        ),
        declaration(
            "huge_int",
            "builtins",
            &["pow"],
            &["i64", "i64"],
            serde_json::json!({}),
            "i64",
            "copied"
        ),
        declaration(
            "wrong_return",
            "math",
            &["sqrt"],
            &["f64"],
            serde_json::json!({}),
            "string",
            "copied"
        )
    ]));
    assert!(matches!(
        registry.call(&bridge, "missing_module", &[], &[]),
        Err(ForeignError::ModuleUnavailable { .. })
    ));
    assert!(matches!(
        registry.call(&bridge, "missing_attribute", &[], &[]),
        Err(ForeignError::AttributeMissing { .. })
    ));
    match registry
        .call(&bridge, "sqrt", &[BridgeValue::F64(-1.0)], &[])
        .unwrap_err()
    {
        ForeignError::PythonException(failure) => {
            assert_eq!(failure.exception_type, "ValueError");
            assert!(!failure.message.is_empty());
            assert!(
                failure.exception_type.len() + failure.message.len() + failure.traceback.len()
                    <= 16_384
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
    match registry
        .call(&bridge, "bad_json", &[BridgeValue::String("{".into())], &[])
        .unwrap_err()
    {
        ForeignError::PythonException(failure) => {
            assert_eq!(failure.exception_type, "JSONDecodeError");
            assert!(failure.traceback.contains("decoder.py"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert!(matches!(
        registry.call(&bridge, "wrong_return", &[BridgeValue::F64(4.0)], &[]),
        Err(ForeignError::ReturnConversion { .. })
    ));
    match registry
        .call(
            &bridge,
            "huge_int",
            &[BridgeValue::I64(2), BridgeValue::I64(100)],
            &[],
        )
        .unwrap_err()
    {
        ForeignError::ReturnConversion {
            failure: Some(failure),
            ..
        } => assert_eq!(failure.exception_type, "OverflowError"),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn return_mismatch_does_not_claim_rollback() {
    let bridge = PythonBridge::new().unwrap();
    let sys = bridge.import_module("sys").unwrap();
    let attribute = "_cgx2_foreign_side_effect_9";
    let registry = registry(serde_json::json!([declaration(
        "set",
        "builtins",
        &["setattr"],
        &["python_handle", "string", "i64"],
        serde_json::json!({}),
        "bool",
        "copied"
    )]));
    assert_eq!(
        registry
            .call(
                &bridge,
                "set",
                &[
                    BridgeValue::Handle(sys.clone()),
                    BridgeValue::String(attribute.into()),
                    BridgeValue::String("wrong".into())
                ],
                &[]
            )
            .unwrap_err()
            .category(),
        "argument"
    );
    assert!(matches!(
        bridge.getattr(&sys, attribute),
        Err(BridgeError::AttributeMissing { .. })
    ));
    assert_eq!(
        registry
            .call(
                &bridge,
                "set",
                &[
                    BridgeValue::Handle(sys.clone()),
                    BridgeValue::String(attribute.into()),
                    BridgeValue::I64(7)
                ],
                &[]
            )
            .unwrap_err()
            .category(),
        "return_conversion"
    );
    let actual = bridge.getattr(&sys, attribute).unwrap();
    assert!(matches!(
        bridge.to_value(&actual).unwrap(),
        BridgeValue::I64(7)
    ));
}

#[test]
fn returned_handle_clone_and_drop_keep_python_references() {
    let bridge = PythonBridge::new().unwrap();
    let registry = registry(serde_json::json!([declaration(
        "counter",
        "collections",
        &["Counter"],
        &[],
        serde_json::json!({"apples":"i64"}),
        "python_handle",
        "python_handle"
    )]));
    let result = registry
        .call(&bridge, "counter", &[], &[("apples", BridgeValue::I64(2))])
        .unwrap();
    let BridgeValue::Handle(handle) = result else {
        panic!("expected handle")
    };
    let sys = bridge.import_module("sys").unwrap();
    let getrefcount = bridge.getattr(&sys, "getrefcount").unwrap();
    let count = || {
        let result = bridge
            .call(&getrefcount, &[BridgeValue::Handle(handle.clone())])
            .unwrap();
        let BridgeValue::I64(n) = bridge.to_value(&result).unwrap() else {
            panic!("expected count")
        };
        n
    };
    let before = count();
    let clone = handle.clone();
    assert_eq!(count(), before + 1);
    drop(clone);
    assert_eq!(count(), before);
}
