use cgx2_python_bridge::{BridgeError, BridgeValue, PythonBridge};

#[test]
fn shape_is_checked_before_a_foreign_allocation() {
    let bridge = PythonBridge::new().unwrap();
    let error = bridge
        .tensor_from_f32(&[1.0, 2.0, 3.0], &[2, 2])
        .unwrap_err();
    assert!(matches!(error, BridgeError::TypeConversion { .. }));
}

#[test]
fn real_torch_cpu_round_trip_or_explicit_skip() {
    let bridge = PythonBridge::new().unwrap();
    match bridge.import_module("torch") {
        Err(BridgeError::ModuleUnavailable { .. }) => {
            eprintln!(
                "SKIPPED real PyTorch integration: torch is absent from the linked Python interpreter"
            );
            assert!(matches!(
                bridge.tensor_from_f32(&[1.0, 2.0], &[2]).unwrap_err(),
                BridgeError::ModuleUnavailable { .. }
            ));
        }
        Err(error) => panic!("torch import failed unexpectedly: {error}"),
        Ok(torch) => {
            let left = bridge.tensor_from_f32(&[1.0, 2.0], &[2]).unwrap();
            let right = bridge.tensor_from_f32(&[3.0, 4.0], &[2]).unwrap();
            assert_eq!(left.shape().unwrap(), vec![2]);
            assert_eq!(left.dtype().unwrap(), "torch.float32");
            assert_eq!(left.device().unwrap(), "cpu");
            let sum = left.add(&right).unwrap();
            assert_eq!(sum.shape().unwrap(), vec![2]);
            let mismatched = bridge.tensor_from_f32(&[1.0, 2.0], &[1, 2]).unwrap();
            assert!(matches!(
                left.add(&mismatched).unwrap_err(),
                BridgeError::TypeConversion { .. }
            ));

            let arange = bridge.getattr(&torch, "arange").unwrap();
            let ints = bridge.call(&arange, &[BridgeValue::I64(2)]).unwrap();
            assert!(matches!(
                bridge.tensor_from_handle(ints).unwrap_err(),
                BridgeError::TypeConversion { .. }
            ));
        }
    }
}
