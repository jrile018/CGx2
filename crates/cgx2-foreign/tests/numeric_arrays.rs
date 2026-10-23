use cgx2_foreign::{BridgeValue, ForeignError, PythonBridge, parse_declarations};

fn registry() -> cgx2_foreign::Registry {
    parse_declarations(&serde_json::json!({"version":1,"calls":[
        {"key":"array_copy","module":"array","attribute":["array"],"positional":["string","f64_array_1d_copy_v1"],"keywords":{},"returns":"f64_array_1d_copy_v1","effect":"Foreign","ownership":"copied"},
        {"key":"list_incompatible","module":"builtins","attribute":["list"],"positional":["f64_array_1d_copy_v1"],"keywords":{},"returns":"f64_array_1d_copy_v1","effect":"Foreign","ownership":"copied"},
        {"key":"bad_typecode","module":"array","attribute":["array"],"positional":["string"],"keywords":{},"returns":"f64_array_1d_copy_v1","effect":"Foreign","ownership":"copied"}
    ]}).to_string()).unwrap()
}

#[test]
fn allowlisted_stdlib_array_roundtrip_and_lifetimes() {
    let bridge = PythonBridge::new().unwrap();
    println!("embedded CPython: {}", bridge.python_version());
    let registry = registry();
    assert!(registry.to_json().contains("f64_array_1d_copy_v1"));
    for _ in 0..128 {
        let input = [
            BridgeValue::String("d".into()),
            BridgeValue::F64Array1DCopyV1(vec![1.25, -3.5]),
        ];
        let copied = registry.call(&bridge, "array_copy", &input, &[]).unwrap();
        assert!(matches!(copied, BridgeValue::F64Array1DCopyV1(values) if values == [1.25, -3.5]));
        assert!(matches!(
            registry.call(
                &bridge,
                "list_incompatible",
                &[BridgeValue::F64Array1DCopyV1(vec![1.0])],
                &[]
            ),
            Err(ForeignError::ReturnConversion { .. })
        ));
        assert!(matches!(
            registry.call(
                &bridge,
                "bad_typecode",
                &[BridgeValue::String("invalid".into())],
                &[]
            ),
            Err(ForeignError::PythonException(_))
        ));
    }
    assert!(matches!(
        registry.call(
            &bridge,
            "array_copy",
            &[
                BridgeValue::String("d".into()),
                BridgeValue::List(vec![BridgeValue::F64(1.0)])
            ],
            &[]
        ),
        Err(ForeignError::Argument(_))
    ));
    assert!(matches!(
        registry.call(
            &bridge,
            "array_copy",
            &[
                BridgeValue::String("d".into()),
                BridgeValue::F64Array1DCopyV1(vec![f64::NAN])
            ],
            &[]
        ),
        Err(ForeignError::Argument(_))
    ));
    assert!(matches!(
        registry.call(&bridge, "not_declared", &[], &[]),
        Err(ForeignError::BlockedCall { .. })
    ));
}

#[test]
fn failed_return_and_exception_release_python_objects() {
    let bridge = PythonBridge::new().unwrap();
    let builtins = bridge.import_module("builtins").unwrap();
    let exec = bridge.getattr(&builtins, "exec").unwrap();
    let source = r#"
import sys, types, array, weakref
m = types.ModuleType('cgx2_array_lifetime_fixture')
def incompatible():
    value = array.array('d', [1.0])
    m.last = weakref.ref(value)
    return [value]
def raising():
    value = array.array('d', [1.0])
    m.last = weakref.ref(value)
    raise ValueError('deliberate')
m.incompatible = incompatible
m.raising = raising
sys.modules[m.__name__] = m
"#;
    bridge
        .call(
            &exec,
            &[
                BridgeValue::String(source.into()),
                BridgeValue::Dict(Default::default()),
            ],
        )
        .unwrap();
    let registry = parse_declarations(&serde_json::json!({"version":1,"calls":[
        {"key":"incompatible","module":"cgx2_array_lifetime_fixture","attribute":["incompatible"],"positional":[],"keywords":{},"returns":"f64_array_1d_copy_v1","effect":"Foreign","ownership":"copied"},
        {"key":"raising","module":"cgx2_array_lifetime_fixture","attribute":["raising"],"positional":[],"keywords":{},"returns":"f64_array_1d_copy_v1","effect":"Foreign","ownership":"copied"}
    ]}).to_string()).unwrap();
    let module = bridge.import_module("cgx2_array_lifetime_fixture").unwrap();
    for _ in 0..128 {
        assert!(matches!(
            registry.call(&bridge, "incompatible", &[], &[]),
            Err(ForeignError::ReturnConversion { .. })
        ));
        let weak = bridge.getattr(&module, "last").unwrap();
        assert!(matches!(
            bridge.to_value(&bridge.call(&weak, &[]).unwrap()).unwrap(),
            BridgeValue::None
        ));
        assert!(matches!(
            registry.call(&bridge, "raising", &[], &[]),
            Err(ForeignError::PythonException(_))
        ));
        let weak = bridge.getattr(&module, "last").unwrap();
        assert!(matches!(
            bridge.to_value(&bridge.call(&weak, &[]).unwrap()).unwrap(),
            BridgeValue::None
        ));
    }
}
