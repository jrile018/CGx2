use cgx2_syntax::{format_module, parse_module, ExprKind, StmtKind};

#[test]
fn parses_and_formats_local_memory_calls_with_nested_guard() {
    let source = "fn build() -> Unit effects(ManagedAlloc):\n  let o = memory.owned_bytes(8)\nfn main() -> Unit effects(ManagedAlloc):\n  no_gc(budget: 64):\n   build()\n";
    let module = parse_module(source).unwrap();
    assert_eq!(module.functions.len(), 2);
    let StmtKind::NoGc { body, .. } = &module.functions[1].body[0].kind else {
        panic!("expected no_gc guard");
    };
    let StmtKind::Expr(call) = &body[0].kind else {
        panic!("expected local call");
    };
    assert!(matches!(call.kind, ExprKind::Call { .. }));
    let formatted = format_module(&module);
    assert!(formatted.contains("    no_gc(budget: 64):\n        build()\n"));
    assert_eq!(format_module(&parse_module(&formatted).unwrap()), formatted);
}
