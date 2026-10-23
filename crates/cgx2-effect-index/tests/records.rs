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
