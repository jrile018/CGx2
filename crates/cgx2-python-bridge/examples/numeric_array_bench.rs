//! Run with `cargo run --release -p cgx2-python-bridge --example numeric_array_bench --offline`.
//! Reports observed embedded-CPython call and copy cost on this host only.
use cgx2_python_bridge::{BridgeValue, PythonBridge};
use std::time::Instant;

fn main() {
    let bridge = PythonBridge::new().expect("linked CPython");
    let operator = bridge.import_module("operator").unwrap();
    let add = bridge.getattr(&operator, "add").unwrap();
    let builtins = bridge.import_module("builtins").unwrap();
    let list = bridge.getattr(&builtins, "list").unwrap();
    let array = bridge.import_module("array").unwrap();
    let array_ctor = bridge.getattr(&array, "array").unwrap();
    let values: Vec<f64> = (0..256).map(f64::from).collect();
    let iterations = 1_000;
    println!(
        "embedded CPython: {}; iterations: {iterations}; array elements: {}",
        bridge.python_version(),
        values.len()
    );
    for (name, run) in [
        ("scalar add", 0),
        ("list copy", 1),
        ("declared array copy", 2),
    ] {
        let start = Instant::now();
        for _ in 0..iterations {
            match run {
                0 => {
                    let result = bridge
                        .call(&add, &[BridgeValue::F64(1.0), BridgeValue::F64(2.0)])
                        .unwrap();
                    let _ = bridge.to_value(&result).unwrap();
                }
                1 => {
                    let result = bridge
                        .call(
                            &list,
                            &[BridgeValue::List(
                                values.iter().copied().map(BridgeValue::F64).collect(),
                            )],
                        )
                        .unwrap();
                    let _ = bridge.to_value(&result).unwrap();
                }
                _ => {
                    let result = bridge
                        .call(
                            &array_ctor,
                            &[
                                BridgeValue::String("d".into()),
                                BridgeValue::F64Array1DCopyV1(values.clone()),
                            ],
                        )
                        .unwrap();
                    let _ = bridge
                        .to_f64_array_1d_copy_v1(&result, values.len() * 8)
                        .unwrap();
                }
            }
        }
        println!(
            "{name}: {:.2} us/call",
            start.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(iterations)
        );
    }
}
