use cgx2_effect_index::{index_source, CallKind, Effect};
use std::collections::BTreeSet;

#[test]
fn checked_contract_calls_keep_nested_repeated_utf8_sites_and_exclude_body_bindings() {
    let source = "# café 🦀\nfn leaf(x: Bool) -> Bool:\n    x\nfn choose(x: Bool) -> Bool:\n    if x:\n        return leaf(x)\n    else:\n        return leaf(false)\npub fn run(x: Bool) -> Bool:\n    requires choose(leaf(x))\n    ensures leaf(result)\n    ensures leaf(result)\n    let leaf = true\n    x\n";
    for syntax_only in [false, true] {
        let index = index_source(source, syntax_only).unwrap();
        let run = index.functions.iter().find(|f| f.name == "run").unwrap();
        let sites: Vec<_> = run
            .calls
            .iter()
            .map(|c| &source[c.span.start..c.span.end])
            .collect();
        assert_eq!(
            sites,
            ["choose(leaf(x))", "leaf(x)", "leaf(result)", "leaf(result)"]
        );
        assert!(run.calls.iter().all(|c| c.kind == CallKind::Local));
        assert!(run
            .calls
            .windows(2)
            .all(|c| c[0].span.start < c[1].span.start));
        assert!(run.direct_effects.is_empty());
        assert!(run.transitive_effects.is_empty());
        let choose = index.functions.iter().find(|f| f.name == "choose").unwrap();
        assert_eq!(choose.calls.len(), 2);
        assert!(choose.calls.iter().all(|c| c.kind == CallKind::Local));
        assert_eq!(
            index.to_json(),
            index_source(source, syntax_only).unwrap().to_json()
        );
    }
}

#[test]
fn result_binding_exists_only_in_postconditions() {
    let source = "fn result() -> Bool:\n    true\nfn run() -> Bool:\n    requires result()\n    ensures result()\n    result()\n";
    for syntax_only in [false, true] {
        let index = index_source(source, syntax_only).unwrap();
        assert_eq!(
            index.functions[1]
                .calls
                .iter()
                .map(|c| c.kind)
                .collect::<Vec<_>>(),
            [CallKind::Local, CallKind::Unknown, CallKind::Local]
        );
        assert_eq!(
            index.functions[1].direct_effects,
            BTreeSet::from([Effect::Unknown])
        );
    }
}

#[test]
fn parameters_and_body_locals_shadow_helpers_in_their_own_scopes() {
    let source = "fn helper() -> Bool:\n    true\nfn parameter(helper: Bool) -> Bool:\n    requires helper()\n    ensures helper()\n    helper()\nfn local() -> Bool:\n    requires helper()\n    ensures helper()\n    let helper = true\n    helper()\n";
    for syntax_only in [false, true] {
        let index = index_source(source, syntax_only).unwrap();
        let parameter = index
            .functions
            .iter()
            .find(|f| f.name == "parameter")
            .unwrap();
        let local = index.functions.iter().find(|f| f.name == "local").unwrap();
        assert!(parameter.calls.iter().all(|c| c.kind == CallKind::Unknown));
        assert_eq!(
            local.calls.iter().map(|c| c.kind).collect::<Vec<_>>(),
            [CallKind::Local, CallKind::Local, CallKind::Unknown]
        );
    }
}

#[test]
fn contract_edges_propagate_effects_and_cycles_without_duplicate_sites() {
    let source = "fn leaf() -> Bool:\n    ffi.call()\nfn run() -> Bool:\n    requires leaf()\n    ensures leaf()\n    ensures result\n    true\nfn root() -> Bool:\n    run()\n";
    assert!(index_source(source, false).is_err());
    let index = index_source(source, true).unwrap();
    let run = index.functions.iter().find(|f| f.name == "run").unwrap();
    let root = index.functions.iter().find(|f| f.name == "root").unwrap();
    assert_eq!(run.calls.len(), 2);
    assert!(run.direct_effects.is_empty());
    let effects = BTreeSet::from([Effect::Foreign, Effect::Unknown]);
    assert_eq!(run.transitive_effects, effects);
    assert_eq!(root.transitive_effects, effects);

    let recursive =
        "fn a() -> Bool:\n    requires b()\n    true\nfn b() -> Bool:\n    ensures a()\n    true\n";
    assert!(index_source(recursive, false).is_err());
    let index = index_source(recursive, true).unwrap();
    assert_eq!(index.cycles, vec![vec!["a".to_owned(), "b".to_owned()]]);
    assert!(index
        .functions
        .iter()
        .all(|f| f.recursive && f.calls.len() == 1));
}

#[test]
fn qualified_contract_calls_stay_unresolved_and_shadowed_foreign_roots_stay_unknown() {
    let source = "import dep\nfn run(ffi: Bool) -> Bool:\n    requires dep.helper()\n    ensures ffi.call()\n    true\n";
    assert!(index_source(source, false).is_err());
    let index = index_source(source, true).unwrap();
    assert_eq!(index.functions[0].calls.len(), 2);
    assert!(index.functions[0]
        .calls
        .iter()
        .all(|c| c.kind == CallKind::Unknown));
    assert_eq!(
        index.functions[0].direct_effects,
        BTreeSet::from([Effect::Unknown])
    );
}
