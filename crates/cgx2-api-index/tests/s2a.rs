use cgx2_syntax::parse_module;
#[test]
fn schema_six_preserves_public_qualified_identities_arguments_and_spans() {
    let source="module app.api\npub variant Reply:\n    Empty\n    Value(Option<Result<i64, Bool>>)\nvariant Secret:\n    Case\npub fn make() -> Reply:\n    Reply.Empty()\n";
    let module = parse_module(source).unwrap();
    let typed = cgx2_sema::check_module(&module).unwrap();
    let index = cgx2_api_index::build_index(&module, &typed).unwrap();
    assert_eq!(index.schema_version, 6);
    assert_eq!(index.variants.len(), 1);
    let variant = &index.variants[0];
    assert_eq!(variant.identity, "app.api.Reply");
    let payload = &variant.cases[1].payloads[0];
    assert_eq!(payload.identity, "cgx2.core.Option");
    assert_eq!(payload.arguments[0].identity, "cgx2.core.Result");
    assert_eq!(
        &source[payload.span.start..payload.span.end],
        "Option<Result<i64, Bool>>"
    );
    assert_eq!(
        index.render_json(),
        cgx2_api_index::build_index(&module, &typed)
            .unwrap()
            .render_json()
    );
    assert!(!index.render_json().contains("Secret"));
}
#[test]
fn syntax_only_api_reports_constraints_without_claiming_semantics() {
    let module =
        parse_module("module app.generic\npub variant Box<T: Eq + Show>:\n    Value(T)\n").unwrap();
    assert!(cgx2_sema::check_module(&module).is_err());
    let index = cgx2_api_index::build_syntax_index(&module);
    assert_eq!(index.semantics_status, "syntax-only");
    assert_eq!(
        index.variants[0].type_parameters[0].constraints,
        ["Eq", "Show"]
    );
    assert!(index.render_json().contains("syntax-only"));
}

#[test]
fn next_stage_array_and_loop_contract_metadata_remains_unchecked() {
    let module=parse_module("pub fn sample<T: Eq>(values: Array<T>) -> T:\n    for i in range(0, 1):\n        invariant i >= 0\n        decreases 1 - i\n        values[i]\n    values[0]\n").unwrap();
    let index = cgx2_api_index::build_syntax_index(&module);
    assert_eq!(index.functions[0].parameter_types[0].identity, "Array");
    assert_eq!(
        index.functions[0].parameter_types[0].arguments[0].identity,
        "T"
    );
    assert_eq!(index.functions[0].loop_contracts.len(), 2);
    assert_eq!(index.functions[0].loop_contracts[0].status, "unchecked");
    assert!(index.render_json().contains("loop_contracts"));
}
