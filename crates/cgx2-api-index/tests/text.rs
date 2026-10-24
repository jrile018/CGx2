use cgx2_api_index::{build_index, build_syntax_index};
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

#[test]
fn checked_text_preserves_nominal_identities_and_effect_statuses() {
    let module = parse_module(include_str!("fixtures/text-checked.cg")).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    assert_eq!(
        index.render_text(),
        include_str!("fixtures/text-checked.txt")
    );
    assert_eq!(
        index.render_json(),
        include_str!("fixtures/text-checked.json").trim_end()
    );
    assert_eq!(
        index.render_text(),
        build_index(&module, &typed).unwrap().render_text()
    );
}

#[test]
fn syntax_text_preserves_generics_nested_types_loop_annotations_and_unchecked_statuses() {
    let module = parse_module(include_str!("fixtures/text-syntax-only.cg")).unwrap();
    assert!(check_module(&module).is_err());
    let index = build_syntax_index(&module);
    assert_eq!(
        index.render_text(),
        include_str!("fixtures/text-syntax-only.txt")
    );
    assert_eq!(
        index.render_json(),
        include_str!("fixtures/text-syntax-only.json").trim_end()
    );
    assert_eq!(
        index.render_text(),
        build_syntax_index(&module).render_text()
    );
}

#[test]
fn contracts_distinguish_checked_and_syntax_only_for_the_same_source() {
    let module = parse_module(include_str!("fixtures/text-contracts.cg")).unwrap();
    let typed = check_module(&module).unwrap();
    let checked = build_index(&module, &typed).unwrap();
    let syntax = build_syntax_index(&module);
    assert_eq!(
        checked.render_text(),
        include_str!("fixtures/text-contracts-checked.txt")
    );
    assert_eq!(
        syntax.render_text(),
        include_str!("fixtures/text-contracts-syntax-only.txt")
    );
    assert_eq!(
        checked.render_json(),
        include_str!("fixtures/text-contracts-checked.json").trim_end()
    );
    assert_eq!(
        syntax.render_json(),
        include_str!("fixtures/text-contracts-syntax-only.json").trim_end()
    );
}

#[test]
fn text_uses_stored_statuses_and_preserves_an_unannotated_result() {
    let module = parse_module(
        "pub fn sample():\n    for i in range(0, 1):\n        invariant true\n        0\n    0\n",
    )
    .unwrap();
    let mut index = build_syntax_index(&module);
    // The renderer must not derive statuses from schema version or semantics mode.
    index.functions[0].loop_contracts[0].status = "assumed".into();
    assert_eq!(
        index.render_text(),
        "module <unnamed> [syntax-only]\npub fn sample() -> <inferred> effects() [unchecked] caps() [unchecked] [0, 82)\n    invariant true [assumed] [51, 66)\n"
    );
}

#[test]
fn syntax_only_foreign_output_uses_the_stored_unsafe_flag_without_a_binding_id() {
    let source = "unsafe foreign native pub fn double(x: i64) -> i64 effects(Foreign) caps(native) = fixture.fixture_double runtime \"native_fixture@1.0.0\" ownership copied\n";
    let module = parse_module(source).unwrap();
    let syntax = build_syntax_index(&module);
    assert!(syntax.functions[0]
        .foreign
        .as_ref()
        .unwrap()
        .binding_id
        .is_none());
    assert!(syntax.render_text().ends_with("    foreign route=native target=fixture.fixture_double ownership=copied runtime=native_fixture@1.0.0 unsafe=true\n"));
    let checked = build_index(&module, &check_module(&module).unwrap()).unwrap();
    assert!(checked.render_text().ends_with("    foreign route=native target=fixture.fixture_double ownership=copied runtime=native_fixture@1.0.0 unsafe=true binding_id=fixture\n"));
}
