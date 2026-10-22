use cgx2_api_index::build_index;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

#[test]
fn tensor_api_index_names_unsafe_bounded_profile() {
    let source = include_str!("../../cgx2-cli/tests/fixtures/tensor-cpu/src/main.cg");
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    let call = index.find("add_f64_cpu").unwrap();
    let foreign = call.foreign.as_ref().unwrap();
    assert!(foreign.unsafe_call);
    assert_eq!(call.effects, ["Foreign"]);
    assert_eq!(call.capabilities, ["native"]);
    assert_eq!(foreign.profile.as_deref(), Some("cpu-1d-f64-add-copied-v1"));
    assert!(index
        .render_json()
        .contains("\"profile\":\"cpu-1d-f64-add-copied-v1\""));
}
