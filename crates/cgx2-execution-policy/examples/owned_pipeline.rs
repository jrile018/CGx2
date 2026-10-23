//! Executable S2 host-adapter fixture, not implemented `.cg` syntax/lowering.
use cgx2_execution_policy::owned_v1::{self, Budgets, Context, Input};
use cgx2_execution_policy::{Backend, Policy, profile::current_machine};
use std::sync::atomic::AtomicBool;

fn main() {
    // Capture once, outside timed dispatch. A compiler adapter must establish
    // source purity/ownership and construct/move the buffers before this call.
    let machine = current_machine().expect("local machine fingerprint");
    let context = Context {
        parallel_workers: 2,
        machine: &machine,
        profile: None,
    };
    let cancellation = AtomicBool::new(false);
    for input in [
        Input::Map {
            values: vec![2; 4096],
            multiplier: 3,
            addend: 7,
        },
        Input::ZipMap {
            left: vec![2; 4096],
            right: vec![3; 4096],
            addend: 7,
        },
    ] {
        let outcome = owned_v1::execute(
            input,
            Policy::Auto,
            context,
            &cancellation,
            Budgets {
                requested_buffer_bytes: Some(4096 * 8 * 3),
                elapsed_ns: None,
            },
        );
        assert_eq!(outcome.result.unwrap(), vec![13; 4096]);
        assert_eq!(
            outcome.report.decision.unwrap().selected,
            Backend::SequentialCpu
        );
        println!("{:?}", outcome.report);
    }
    println!("source_pipeline_fixture=pass library_only=true");
}
