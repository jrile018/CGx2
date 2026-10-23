//! S2 consumer fixture: checked scalar kernels, Rust-owned inputs, existing policy
//! library. This deliberately does not claim source-level collection lowering.
use cgx2_cli::{Value, run_checked_source};
use cgx2_execution_policy::{Backend, Capabilities, Policy, execute_map, execute_zip_map};
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

const KERNELS: &str = include_str!("fixtures/release-core/pipeline.cg");

#[test]
fn checked_owned_input_map_zip_consumer_contract() {
    let module = parse_module(KERNELS).unwrap();
    let checked = check_module(&module).unwrap();
    for name in ["map_one", "zip_one"] {
        assert!(checked.effects[name].effects.is_empty());
        assert!(checked.effects[name].capabilities.is_empty());
    }
    // Ownership is explicit at the future compiler/runtime boundary. No managed
    // handles, aliases into a managed session, or external effects are admitted.
    let input = vec![0, 1, 4, -2];
    let right = vec![10, 20, 30, 40];
    let caps = Capabilities::default();
    let mapped = execute_map(&input, 3, 1, Policy::Auto, &caps).unwrap();
    assert_eq!(mapped.decision.selected, Backend::SequentialCpu);
    let zipped = execute_zip_map(&mapped.values, &right, 2, Policy::Auto, &caps).unwrap();
    assert_eq!(zipped.decision.selected, Backend::SequentialCpu);
    for ((left, right), result) in input.into_iter().zip(right).zip(zipped.values) {
        let program = KERNELS.replace(
            "zip_one(map_one(4), 10)",
            &format!("zip_one(map_one({left}), {right})"),
        );
        assert_eq!(
            run_checked_source(&program, &[]).unwrap().0.value,
            Value::I64(result)
        );
    }
    assert!(execute_map(&[i64::MAX], 3, 1, Policy::Auto, &caps).is_err());
    assert!(execute_zip_map(&[1], &[], 2, Policy::Auto, &caps).is_err());
    assert!(execute_zip_map(&[i64::MAX], &[1], 2, Policy::Auto, &caps).is_err());
}
