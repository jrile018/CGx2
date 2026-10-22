use cgx2_python_bridge::{BridgeError, BridgeValue, PythonBridge, TorchStatus};

#[test]
fn torch_probe_reports_real_capability_or_missing_package() {
    let bridge = PythonBridge::new().unwrap();
    let probe = bridge.probe_torch();
    assert_eq!(probe.python_version, bridge.python_version());
    assert_eq!(probe.python_prefix, bridge.python_prefix());
    eprintln!(
        "linked CPython: {} | prefix: {}",
        probe.python_version, probe.python_prefix
    );
    match probe.status {
        TorchStatus::Available { torch_version } => assert!(!torch_version.is_empty()),
        TorchStatus::Unavailable { failure } => {
            eprintln!(
                "SKIPPED real PyTorch capability test: torch is absent from the linked Python interpreter"
            );
            assert_eq!(failure.exception_type, "ModuleNotFoundError");
        }
        TorchStatus::Broken { error, .. } => {
            panic!("installed torch failed capability probe: {error}")
        }
    }
}

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
