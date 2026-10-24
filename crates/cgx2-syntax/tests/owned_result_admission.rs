use cgx2_syntax::{ordinary_owned_span, parse_module};

#[test]
fn guard_only_legacy_unit_effects_preserve_original_admission_path() {
    let source = "fn main() -> Unit effects():\n    no_gc(budget: 0):\n        return\n";
    assert!(ordinary_owned_span(&parse_module(source).unwrap()).is_none());
}

#[test]
fn ordinary_copied_result_guard_uses_checked_new_profile() {
    let source = "fn main() -> i64:\n    no_gc(budget: 0):\n        return 42\n";
    assert!(ordinary_owned_span(&parse_module(source).unwrap()).is_some());
}

#[test]
fn resolved_try_owned_functions_and_records_keep_copied_admission() {
    for source in [
        "fn try_owned(Result: i64) -> i64:\n    Result\nfn main() -> i64:\n    try_owned(42)\n",
        "record try_owned:\n    AllocationError: i64\nfn main() -> i64:\n    let Result = try_owned(AllocationError: 42)\n    Result.AllocationError\n",
        include_str!("../owned-symbol-compatibility-evidence/fixtures/plain-symbols.cg"),
    ] {
        assert!(ordinary_owned_span(&parse_module(source).unwrap()).is_none());
    }
}

#[test]
fn unresolved_intrinsic_and_wrapped_old_names_still_select_ownership() {
    for source in [
        "record R:\n    x: i64\nfn main() -> i64:\n    try_owned(R(x: 42))\n    42\n",
        include_str!("../owned-symbol-compatibility-evidence/fixtures/nominal-result.cg"),
        include_str!("../owned-symbol-compatibility-evidence/fixtures/user-function.cg"),
        include_str!("../owned-symbol-compatibility-evidence/fixtures/nominal-try-owned.cg"),
    ] {
        assert!(ordinary_owned_span(&parse_module(source).unwrap()).is_some());
    }
}
