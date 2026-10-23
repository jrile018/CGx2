use cgx2_api_index::build_index;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

#[test]
fn record_declarations_and_field_types_are_indexed() {
    let module = parse_module("module sample\npub record Point:\n    x: i64\n    active: Bool\npub fn read(p: Point) -> i64:\n    p.x\n").unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    assert_eq!(index.schema_version, 6);
    assert_eq!(index.records[0].name, "Point");
    assert_eq!(index.records[0].fields[1].ty, "Bool");
    assert_eq!(index.functions[0].parameters[0].ty, "sample.Point");
    assert!(index
        .render_json()
        .contains("\"records\":[{\"name\":\"Point\""));
    assert!(index.render_text().contains("record Point"));
}
