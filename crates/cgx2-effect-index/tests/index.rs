use cgx2_effect_index::{index_source, CallKind, Effect};

#[test]
fn pure_nested_chain_and_top_level_are_indexed() {
    let source = "fn leaf(x: i64) -> i64:\n    x + 1\nfn middle(x: i64) -> i64:\n    leaf(x)\npub fn root(x: i64) -> i64:\n    middle(leaf(x))\n";
    let index = index_source(source, false).unwrap();
    assert_eq!(
        index
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["leaf", "middle", "root"]
    );
    assert_eq!(index.functions[2].calls.len(), 2);
    assert!(index
        .functions
        .iter()
        .all(|f| f.transitive_effects.is_empty()));
    assert!(index.top_level.transitive_effects.is_empty());
    assert_eq!(index.functions[2].parameters[0].ty, "i64");
    assert_eq!(index.functions[2].return_type.as_deref(), Some("i64"));
}

#[test]
fn unknown_and_foreign_calls_propagate_in_syntax_only_mode() {
    let source = "fn leaf() -> i64:\n    ffi.call()\nfn root() -> i64:\n    leaf()\n";
    assert!(index_source(source, false).is_err());
    let index = index_source(source, true).unwrap();
    assert_eq!(index.functions[0].calls[0].kind, CallKind::Foreign);
    for effects in [
        &index.functions[0].direct_effects,
        &index.functions[1].transitive_effects,
    ] {
        assert!(effects.contains(&Effect::Unknown));
    }
    assert!(index.functions[0].direct_effects.contains(&Effect::Foreign));
}

#[test]
fn memory_and_mutation_include_top_level_effects() {
    let source = "let r = memory.region(mode: manual, limit: 256)\nlet x = memory.alloc_bytes(r, 4)\nmemory.drop(x)\nmemory.collect(r)\nmemory.close(r)\n";
    let index = index_source(source, false).unwrap();
    assert!(index
        .top_level
        .direct_effects
        .contains(&Effect::ManagedAlloc));
    assert!(index.top_level.direct_effects.contains(&Effect::Collect));
    assert!(index.top_level.direct_effects.contains(&Effect::Mutate));
    assert!(!index.top_level.direct_effects.contains(&Effect::Unknown));
}

#[test]
fn mutual_recursion_terminates_and_json_is_deterministic() {
    let source = "fn b(x: i64) -> i64:\n    a(x)\nfn a(x: i64) -> i64:\n    b(x)\n";
    let first = index_source(source, true).unwrap();
    let second = index_source(source, true).unwrap();
    assert_eq!(first.to_json(), second.to_json());
    assert_eq!(first.cycles, vec![vec!["a".to_owned(), "b".to_owned()]]);
    assert!(first.functions.iter().all(|f| f.recursive));
}

#[test]
fn generated_large_cycle_propagates_monotonically() {
    let mut source = String::new();
    for i in 0..160 {
        source.push_str(&format!(
            "fn f{i}(x: i64) -> i64:\n{}    f{}(x)\n",
            if i == 0 { "    unknown()\n" } else { "" },
            (i + 1) % 160
        ));
    }
    source.push_str("fn unknown() -> i64:\n    mystery()\n");
    let index = index_source(&source, true).unwrap();
    assert_eq!(index.cycles[0].len(), 160);
    assert!(index
        .functions
        .iter()
        .all(|f| f.transitive_effects.contains(&Effect::Unknown)));
}

#[test]
fn checked_errors_have_distinct_kinds_and_byte_spans() {
    let parse = index_source("fn bad(:\n", false).unwrap_err();
    assert_eq!(parse[0].stage, "parse");
    let check = index_source("missing()\n", false).unwrap_err();
    assert_eq!(check[0].stage, "check");
    assert_eq!(check[0].span.start, 0);
    assert!(check[0].span.end > 0);
    let index = index_source("missing()\n", true).unwrap();
    assert_eq!(index.top_level.calls[0].kind, CallKind::Unknown);
    assert!(index.top_level.direct_effects.contains(&Effect::Unknown));
}

#[test]
fn read_args_and_shadowed_callee_are_conservative() {
    let checked = index_source("arg_count()\n", false).unwrap();
    assert!(checked.top_level.direct_effects.contains(&Effect::ReadArgs));
    let shadowed = index_source("fn call(arg: i64) -> i64:\n    arg(0)\n", true).unwrap();
    assert_eq!(shadowed.functions[0].calls[0].kind, CallKind::Unknown);
    assert!(shadowed.functions[0]
        .direct_effects
        .contains(&Effect::Unknown));
    let unresolved_value = index_source("unbound\n", true).unwrap();
    assert!(unresolved_value
        .top_level
        .direct_effects
        .contains(&Effect::Unknown));
    let unresolved_mode_word = index_source("manual\n", true).unwrap();
    assert!(unresolved_mode_word
        .top_level
        .direct_effects
        .contains(&Effect::Unknown));
    let memory_function = index_source("fn f() -> i64:\n    memory.use(x)\n", true).unwrap();
    assert!(memory_function.functions[0]
        .direct_effects
        .contains(&Effect::Unknown));
}
