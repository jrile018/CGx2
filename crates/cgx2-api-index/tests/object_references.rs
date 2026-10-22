use cgx2_api_index::{build_index, IndexError};
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

#[test]
fn indexes_checked_public_graph_function() {
    let source = include_str!("../../cgx2-cli/tests/fixtures/object_function.cg");
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    assert_eq!(index.functions.len(), 1);
    assert_eq!(index.functions[0].name, "graph");
    assert_eq!(index.functions[0].return_type, "Unit");
    assert_eq!(
        index.functions[0].effects,
        vec!["ManagedAlloc", "Collect", "Mutate"]
    );
    assert!(index.render_json().contains("\"name\":\"graph\""));
}

#[test]
fn rejects_invalid_graph_even_with_previously_checked_summary() {
    let valid = parse_module(include_str!(
        "../../cgx2-cli/tests/fixtures/object_function.cg"
    ))
    .unwrap();
    let typed = check_module(&valid).unwrap();
    let invalid = parse_module("pub fn graph() -> Unit effects(ManagedAlloc):\n    let r = memory.region(mode: manual, limit: 1024)\n    let a = memory.alloc_object(r, 0, 1)\n    let bad = memory.reference(a, 1)\nfn main() -> Unit effects(ManagedAlloc):\n    graph()\n").unwrap();
    assert!(matches!(
        build_index(&invalid, &typed),
        Err(IndexError::InvalidModule(_))
    ));
}
