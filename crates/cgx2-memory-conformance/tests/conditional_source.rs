//! Independent small accounting oracle: each selected managed byte allocation
//! charges SLOT_METADATA_BYTES + payload. Dropping its root does not refund a
//! no_gc budget or collect a manual region.
use cgx2_cli::{run_checked_source, run_checked_source_with_failure};
use cgx2_runtime::SLOT_METADATA_BYTES;

#[test]
fn selected_paths_charge_cumulatively_across_regions_at_budget_boundary() {
    for payload in 0..16 {
        let charge = SLOT_METADATA_BYTES + payload;
        for selected in [false, true] {
            let body = format!("let a = memory.region(mode: manual, limit: 1024)\nlet b = memory.region(mode: manual, limit: 1024)\nno_gc(budget: BUDGET):\n    if {selected}:\n        let temp = memory.alloc_bytes(a, {payload})\n        memory.drop(temp)\n    else:\n        let temp = memory.alloc_bytes(b, {payload})\n        memory.drop(temp)\n    if {selected}:\n        let temp = memory.alloc_bytes(b, {payload})\n        memory.drop(temp)\n    else:\n        let temp = memory.alloc_bytes(a, {payload})\n        memory.drop(temp)\nmemory.close(a)\nmemory.close(b)\n");
            let success = body.replace("BUDGET", &(2 * charge).to_string());
            let (_, Some(report)) = run_checked_source(&success, &[]).unwrap() else {
                panic!()
            };
            assert_eq!(report.totals.collection_count, 0);
            assert_eq!(report.totals.committed_bytes, 0);
            assert_eq!(report.totals.allocation_failures, 0);
            for region in report.regions.iter().skip(1) {
                assert_eq!(region.peak_bytes, charge);
                assert!(!region.is_open);
            }
            let failure_source = body.replace("BUDGET", &(2 * charge - 1).to_string());
            let failure = run_checked_source_with_failure(&failure_source, &[])
                .err()
                .unwrap()
                .remove(0);
            assert_eq!(failure.error.code, "CGR013");
            assert!(failure.error.message.contains("BudgetExceeded"));
            assert!(failure.cleanup_errors.is_empty());
            let operation = failure.operation_failure.unwrap();
            assert_eq!(operation.memory.totals.committed_bytes, charge);
            assert_eq!(operation.memory.totals.allocation_failures, 1);
            assert_eq!(operation.memory.totals.collection_count, 0);
            assert_eq!(operation.runtime_root_registrations, Some(0));
            assert_eq!(
                failure.final_unwind.unwrap().runtime_root_registrations,
                Some(0)
            );
        }
    }
}
