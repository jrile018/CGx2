use cgx2_effect_index::{index_source, CallKind, Effect};

#[test]
fn checked_record_construction_is_pure() {
    let source = "record Point:\n    x: i64\nfn main() -> i64:\n    let p = Point(x: 3)\n    p.x\n";
    let index = index_source(source, false).unwrap();
    let function = index.functions.iter().find(|f| f.name == "main").unwrap();
    assert!(function.direct_effects.is_empty());
    assert_eq!(function.calls[0].kind, CallKind::Builtin);
    assert!(!function.transitive_effects.contains(&Effect::Unknown));
}

#[test]
fn conditionals_index_guard_and_both_branches_with_separate_scopes() {
    let source = "fn guard() -> Bool:\n    true\nfn left() -> i64:\n    arg_count()\nfn right() -> i64:\n    7\nfn choose() -> i64:\n    if guard():\n        let local = left()\n        return local\n    else:\n        return right()\n";
    let index = index_source(source, false).unwrap();
    let function = index.functions.iter().find(|f| f.name == "choose").unwrap();
    assert_eq!(function.calls.len(), 3);
    assert!(function.transitive_effects.contains(&Effect::ReadArgs));
}
