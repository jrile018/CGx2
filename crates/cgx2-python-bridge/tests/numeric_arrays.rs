use cgx2_python_bridge::{BridgeError, BridgeValue, MAX_F64_ARRAY_ELEMENTS, PythonBridge};

fn eval(bridge: &PythonBridge, expression: &str) -> cgx2_python_bridge::PyHandle {
    let builtins = bridge.import_module("builtins").unwrap();
    let eval = bridge.getattr(&builtins, "eval").unwrap();
    bridge
        .call(
            &eval,
            &[
                BridgeValue::String(expression.into()),
                BridgeValue::Dict(Default::default()),
            ],
        )
        .unwrap()
}

#[test]
fn stdlib_array_copy_and_bounds() {
    let bridge = PythonBridge::new().unwrap();
    println!("embedded CPython: {}", bridge.python_version());
    let value = eval(&bridge, "__import__('array').array('d', [1.25, -2.5])");
    assert_eq!(
        bridge.to_f64_array_1d_copy_v1(&value, 0).unwrap(),
        vec![1.25, -2.5]
    );
    let empty = eval(&bridge, "__import__('array').array('d')");
    assert!(
        bridge
            .to_f64_array_1d_copy_v1(&empty, 0)
            .unwrap()
            .is_empty()
    );
    let values = vec![0.5; MAX_F64_ARRAY_ELEMENTS];
    let array = bridge.import_module("array").unwrap();
    let ctor = bridge.getattr(&array, "array").unwrap();
    let owned = bridge
        .call(
            &ctor,
            &[
                BridgeValue::String("d".into()),
                BridgeValue::F64Array1DCopyV1(values),
            ],
        )
        .unwrap();
    let output = bridge.to_f64_array_1d_copy_v1(&owned, 8_388_608).unwrap();
    assert_eq!(output.len(), MAX_F64_ARRAY_ELEMENTS);
    assert_eq!(output[0], 0.5);
    assert!(matches!(
        bridge.call(
            &ctor,
            &[
                BridgeValue::String("d".into()),
                BridgeValue::F64Array1DCopyV1(vec![0.0; MAX_F64_ARRAY_ELEMENTS + 1])
            ]
        ),
        Err(BridgeError::TypeConversion { .. })
    ));
}

#[test]
fn invalid_buffers_have_stable_conversion_class() {
    let bridge = PythonBridge::new().unwrap();
    for expression in [
        "[1.0, 2.0]",
        "__import__('array').array('f', [1.0])",
        "memoryview(__import__('array').array('d', [1.,2.,3.,4.]))[::2]",
        "memoryview(__import__('array').array('d', [1.,2.,3.,4.])).cast('B').cast('d', (2,2))",
        "__import__('array').array('d', [float('nan')])",
        "__import__('array').array('d', [float('inf')])",
        "__import__('array').array('d', [0.0]) * 1048577",
    ] {
        let value = eval(&bridge, expression);
        assert!(
            matches!(
                bridge.to_f64_array_1d_copy_v1(&value, 0),
                Err(BridgeError::TypeConversion { .. })
            ),
            "{expression}"
        );
    }
    assert!(matches!(
        bridge.to_f64_array_1d_copy_v1(
            &eval(&bridge, "__import__('array').array('d', [1.])"),
            16_777_216
        ),
        Err(BridgeError::TypeConversion { .. })
    ));
}

#[test]
fn numpy_optional_native_and_foreign_endian() {
    let bridge = PythonBridge::new().unwrap();
    let numpy = match bridge.import_module("numpy") {
        Ok(numpy) => numpy,
        Err(BridgeError::ModuleUnavailable { failure, .. })
            if failure.message == "No module named 'numpy'" =>
        {
            println!(
                "SKIPPED optional NumPy fixture: not installed in embedded CPython {}",
                bridge.python_version()
            );
            return;
        }
        Err(error) => panic!("NumPy import failed: {error}"),
    };
    let version = bridge.getattr(&numpy, "__version__").unwrap();
    println!(
        "embedded CPython: {}; NumPy: {:?}",
        bridge.python_version(),
        bridge.to_value(&version).unwrap()
    );
    let native = eval(
        &bridge,
        "__import__('numpy').array([1.0, 2.0], dtype='=f8')",
    );
    assert_eq!(
        bridge.to_f64_array_1d_copy_v1(&native, 0).unwrap(),
        vec![1.0, 2.0]
    );
    let opposite = if cfg!(target_endian = "little") {
        ">f8"
    } else {
        "<f8"
    };
    let foreign = eval(
        &bridge,
        &format!("__import__('numpy').array([1.0], dtype='{opposite}')"),
    );
    assert!(matches!(
        bridge.to_f64_array_1d_copy_v1(&foreign, 0),
        Err(BridgeError::TypeConversion { .. })
    ));
}
