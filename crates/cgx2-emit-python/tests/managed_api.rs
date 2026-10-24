use cgx2_emit_python::emit_python;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

#[test]
fn public_ast_api_preserves_managed_target_rejection() {
    let source = "fn main() -> i64 effects(ManagedAlloc):\n    let r = managed.ref(42)\n    42\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let errors = emit_python(&module, &typed).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "CGE006");
    assert_eq!(errors[0].span, module.span);
}
