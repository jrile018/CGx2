use cgx2_python_bridge::{BridgeError, BridgeValue, PythonBridge};

#[test]
fn imports_and_calls_math_sqrt() {
    let bridge = PythonBridge::new().unwrap();
    assert!(!bridge.python_version().is_empty());
    assert!(!bridge.python_prefix().is_empty());
    let math = bridge.import_module("math").unwrap();
    let sqrt = bridge.getattr(&math, "sqrt").unwrap();
    let result = bridge.call(&sqrt, &[BridgeValue::F64(9.0)]).unwrap();
    assert!(matches!(
        bridge.to_value(&result).unwrap(),
        BridgeValue::F64(3.0)
    ));
}

#[test]
fn unsupported_python_objects_remain_handles() {
    let bridge = PythonBridge::new().unwrap();
    let sys = bridge.import_module("sys").unwrap();
    let info = bridge.getattr(&sys, "version_info").unwrap();
    assert!(matches!(
        bridge.to_value(&info).unwrap(),
        BridgeValue::Handle(_)
    ));
}

#[test]
fn missing_module_and_attribute_are_distinct() {
    let bridge = PythonBridge::new().unwrap();
    let missing = bridge.import_module("cgx2_missing_module_xyz").unwrap_err();
    assert!(matches!(missing, BridgeError::ModuleUnavailable { .. }));
    let math = bridge.import_module("math").unwrap();
    let missing = bridge
        .getattr(&math, "cgx2_missing_attribute_xyz")
        .unwrap_err();
    assert!(matches!(missing, BridgeError::AttributeMissing { .. }));
}

#[test]
fn invalid_call_and_python_exception_keep_diagnostics() {
    let bridge = PythonBridge::new().unwrap();
    let math = bridge.import_module("math").unwrap();
    let sqrt = bridge.getattr(&math, "sqrt").unwrap();
    let error = bridge
        .call(&sqrt, &[BridgeValue::String("bad".into())])
        .unwrap_err();
    assert!(matches!(error, BridgeError::PythonException(_)));
    let json = bridge.import_module("json").unwrap();
    let loads = bridge.getattr(&json, "loads").unwrap();
    let error = bridge
        .call(&loads, &[BridgeValue::String("{".into())])
        .unwrap_err();
    let BridgeError::PythonException(failure) = error else {
        panic!("expected Python exception");
    };
    assert_eq!(failure.exception_type, "JSONDecodeError");
    assert!(failure.message.contains("Expecting"));
    assert!(
        failure.traceback.contains("json/decoder.py")
            || failure.traceback.contains("json\\decoder.py")
    );
}

#[test]
fn primitive_round_trip_and_integer_overflow_is_conversion_error() {
    let bridge = PythonBridge::new().unwrap();
    let builtins = bridge.import_module("builtins").unwrap();
    let tuple = bridge.getattr(&builtins, "tuple").unwrap();
    let empty = bridge.call(&tuple, &[]).unwrap();
    assert!(matches!(
        bridge.to_value(&empty).unwrap(),
        BridgeValue::Handle(_)
    ));
    let int = bridge.getattr(&builtins, "int").unwrap();
    let value = bridge.call(&int, &[BridgeValue::I64(42)]).unwrap();
    assert!(matches!(
        bridge.to_value(&value).unwrap(),
        BridgeValue::I64(42)
    ));
    let none = bridge.getattr(&builtins, "None").unwrap();
    assert!(matches!(bridge.to_value(&none).unwrap(), BridgeValue::None));
    let bool_type = bridge.getattr(&builtins, "bool").unwrap();
    let value = bridge.call(&bool_type, &[BridgeValue::Bool(true)]).unwrap();
    assert!(matches!(
        bridge.to_value(&value).unwrap(),
        BridgeValue::Bool(true)
    ));
    let str_type = bridge.getattr(&builtins, "str").unwrap();
    let value = bridge
        .call(&str_type, &[BridgeValue::String("cgx2".into())])
        .unwrap();
    assert!(matches!(bridge.to_value(&value).unwrap(), BridgeValue::String(s) if s == "cgx2"));
    let pow = bridge.getattr(&builtins, "pow").unwrap();
    let huge = bridge
        .call(&pow, &[BridgeValue::I64(2), BridgeValue::I64(100)])
        .unwrap();
    assert!(matches!(
        bridge.to_value(&huge).unwrap_err(),
        BridgeError::TypeConversion { .. }
    ));
}

#[test]
fn cloned_and_dropped_handles_keep_a_stable_python_refcount() {
    let bridge = PythonBridge::new().unwrap();
    let builtins = bridge.import_module("builtins").unwrap();
    let object = bridge.getattr(&builtins, "object").unwrap();
    let target = bridge.call(&object, &[]).unwrap();
    let sys = bridge.import_module("sys").unwrap();
    let getrefcount = bridge.getattr(&sys, "getrefcount").unwrap();
    let count = || {
        let result = bridge
            .call(&getrefcount, &[BridgeValue::Handle(target.clone())])
            .unwrap();
        let BridgeValue::I64(count) = bridge.to_value(&result).unwrap() else {
            panic!("expected count");
        };
        count
    };
    let initial = count();
    for _ in 0..1000 {
        let temporary = target.clone();
        assert!(count() > initial);
        drop(temporary);
        assert_eq!(count(), initial);
    }
}
