use cgx2_syntax::parse_module;
#[test]
fn schema_seven_records_deterministic_checked_instantiations() {
    let source="module api\npub fn id<T: Eq + Show>(x: T) -> T effects() caps():\n    x\npub fn run() -> String effects() caps():\n    id<String>(\"é\")\n";
    let module = parse_module(source).unwrap();
    let typed = cgx2_sema::check_module(&module).unwrap();
    let index = cgx2_api_index::build_index(&module, &typed).unwrap();
    assert_eq!(index.schema_version, 7);
    assert_eq!(index.instantiations.len(), 1);
    let instance = &index.instantiations[0];
    assert_eq!(instance.callee, "api.id");
    assert_eq!(instance.type_arguments, ["String"]);
    assert_eq!(instance.parameters, ["String"]);
    assert_eq!(instance.return_type, "String");
    assert_eq!(
        &source[instance.span.start..instance.span.end],
        "id<String>(\"é\")"
    );
    let again =
        cgx2_api_index::build_index(&module, &cgx2_sema::check_module(&module).unwrap()).unwrap();
    assert_eq!(index.render_json(), again.render_json());
    assert!(index
        .render_text()
        .contains("instantiate api.id<String>(String) -> String [checked]"));
    let syntax = cgx2_api_index::build_syntax_index(&module);
    assert_eq!(syntax.semantics_status, "syntax-only");
    assert!(syntax.instantiations.is_empty());
    assert_eq!(syntax.functions[0].effects_status, "unchecked");
}

#[test]
fn field_type_strings_do_not_capture_same_spelled_nominal_names() {
    let module = parse_module(
        "module api\npub record T:\n    value: i64\npub record Box<T>:\n    value: Option<T>\n",
    )
    .unwrap();
    let typed = cgx2_sema::check_module(&module).unwrap();
    for index in [
        cgx2_api_index::build_index(&module, &typed).unwrap(),
        cgx2_api_index::build_syntax_index(&module),
    ] {
        let record = index.records.iter().find(|r| r.name == "Box").unwrap();
        assert_eq!(record.fields[0].ty, "Option<T>");
        assert_eq!(record.field_types[0].arguments[0].identity, "T");
    }
}
