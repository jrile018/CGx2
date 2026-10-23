"""Finite empirical acceptance inventory; harness observations are separate gates."""

# Each owner assessment must address every listed requirement with raw evidence.
GATES = {
    "isolation": ("R14", "Scored positive/adversarial isolation", ("all_cases",)),
    "submission": ("R14", "Isolated hidden-test submission", ("hidden_tests",)),
    "public-context": ("R14", "Public-only context export", ("no_private_material",)),
    "task-study": ("R14", "Reviewed holdout and paired model value", ("paired_holdout",)),
    "syntax-observation": ("R14", "Public syntax pilot trace intake", ("validated_trace",)),
    "syntax-study": ("R14", "Blinded paired syntax decision", ("human_assessment", "two_families", "paired_trials")),
    "native-observation": ("R13", "Native/worker calibrated observations", ("raw_pairs",)),
    "libtorch-observation": ("R08/R13", "Native LibTorch observations", ("raw_runs",)),
    "memory-safety": ("R03/R13", "Supported memory corpus and independent audit", ("source_and_runtime", "invalid_programs", "fault_cancel_unwind", "independent_audit")),
    "manual-no-gc": ("R03", "Manual and no_gc collection invariants", ("manual_explicit_only", "zero_protected_collections", "failure_and_nested_paths")),
    "regions-foreign": ("R03/R06", "Region roots, transfers and foreign lifetimes", ("cross_region_roots", "release_aliasing", "foreign_memory_separate")),
    "leak-check": ("R03/R13", "Leak-enabled external host assurance", ("leak_detection_enabled", "full_supported_corpus", "no_leaks")),
    "functional": ("R01-R12", "Source-level release conformance", ("source_semantics", "cli", "packages", "notebooks_once", "cross_target", "security", "documentation")),
    "trusted-solver": ("R11", "Pinned trusted solver acceptance", ("solver_identity", "source_smt_audit", "valid_proofs", "false_witnesses", "unknown_preserved")),
    "managed-resources": ("R13", "Managed vs tuned OCaml resources", ("equivalent_workload", "rss_le_2", "throughput_ge_0_70", "uncertainty", "calibration_frozen")),
    "native-resources": ("R13", "Native vs optimized Rust resources", ("equivalent_workload", "median_le_1_25", "rss_le_1_5", "uncertainty", "calibration_frozen")),
    "libtorch-inference": ("R08/R13", "Compute-dominant inference vs C++ and Python", ("real_inference", "cpp_and_python_separate", "throughput_ge_0_90", "p95_le_1_15", "no_libpython", "device_dtype_pins", "synchronized", "uncertainty")),
    "native-ffi": ("R06/R13", "Checked C ABI vs native wrapper", ("latency_le_1_25", "correctness_copies", "version_rejection", "calibration_frozen", "uncertainty")),
    "python-bridge": ("R07/R13", "Explicit Python bridge vs PyO3", ("overhead_le_1_25", "environment_pinned", "separate_native_route", "calibration_frozen", "uncertainty")),
    "dispatch": ("R12/R13", "Supported automatic execution selection", ("sequential_reference", "effects_ownership_no_gc", "full_dispatch_cost", "small_large_inputs", "uncertainty_disables_auto")),
    "human-comprehension": ("R14", "Genuine blinded human comprehension", ("genuine_participants", "blinded_rubric", "bug_location_top1_top3", "invariant_impact_answers", "time_tokens", "independent_review")),
    "generated-source": ("R10/R14", "Blinded Rust/Python edit and readability study", ("both_targets", "output_error_parity", "genuine_maintainers", "blinded_edit_tasks", "readability", "independent_review")),
    "cli-notebook-latency": ("R05/R09/R13", "Separate cold/warm command and cell latency", ("check_build_run_cells", "cold_warm_separate", "source_tool_hashes", "independent_processes", "uncertainty")),
}

ADAPTERS = {
    "isolation-probe": ("scored_candidate.py", "isolation"),
    "isolation-selftest": ("isolation_selftest.py", "isolation"),
    "score": ("scored_candidate.py", "submission"),
    "public-export": ("syntax_export.py", "public-context"),
    "task-study": ("task_evidence.py", "task-study"),
    "syntax-trace": ("syntax_trial.py", "syntax-observation"),
    "native-runtime": ("native_worker_baselines.py", "native-observation"),
    "libtorch-runtime": ("matrix_runtime.py", "libtorch-observation"),
}
TIMING = {"native-runtime", "libtorch-runtime", "managed-resources", "native-resources",
          "libtorch-inference", "native-ffi", "python-bridge", "dispatch", "cli-notebook-latency"}
