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
