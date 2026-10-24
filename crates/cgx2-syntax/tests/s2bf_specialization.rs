use cgx2_syntax::{canonical_module, parse_module, qualify_type, ExprKind};
use std::collections::BTreeSet;

#[test]
fn qualified_paths_in_both_representations_are_idempotent_recursively() {
    let mut module = parse_module("module out.main\nrecord Long:\n    x: String\nvariant Reply:\n    Item(Long)\nfn main() -> List<Option<Reply>>:\n    []\n").unwrap();
    for lowered_declarations in [false, true] {
        if lowered_declarations {
            module.records[0].name = "out.main.Long".into();
            module.variants[0].name = "out.main.Reply".into();
        }
        for split in [false, true] {
            let mut ty = module.functions[0].return_type.clone().unwrap();
            let nested = &mut ty.arguments[0].arguments[0];
            nested.path.segments = if split {
                vec!["out".into(), "main".into(), "Reply".into()]
            } else {
                vec!["out.main.Reply".into()]
            };
            let expected = ty.clone();
            qualify_type(&mut ty, &module);
            qualify_type(&mut ty, &module);
            assert_eq!(ty, expected);
        }
    }
}

#[test]
fn lowered_constructor_names_do_not_gain_another_module_prefix() {
    let module = parse_module(
        "module out.main\nrecord Long:\n    x: String\nfn main() -> Long:\n    Long(x: \"é\")\n",
    )
    .unwrap();
    let mut canonical = canonical_module(&module, &BTreeSet::new());
    canonical.records[0].name = "out.main.Long".into();
    assert_eq!(canonical_module(&canonical, &BTreeSet::new()), canonical);
    let cgx2_syntax::StmtKind::Expr(expr) = &canonical.functions[0].body[0].kind else {
        panic!("expected expression");
    };
    let ExprKind::Call { callee, .. } = &expr.kind else {
        panic!("expected record call");
    };
    let ExprKind::Name(path) = &callee.kind else {
        panic!("expected canonical name");
    };
    assert_eq!(path.segments, ["out.main.Long"]);
}

#[test]
fn local_names_qualify_once_and_binders_keep_their_scope() {
    let module = parse_module("module T\nrecord T:\n    value: i64\npub record Box<T>:\n    value: List<T>\npub variant Choice<T>:\n    Item(Array<T>)\nfn main() -> T:\n    T(value: 1)\n").unwrap();
    let canonical = canonical_module(&module, &BTreeSet::new());
    assert_eq!(canonical_module(&canonical, &BTreeSet::new()), canonical);
    assert_eq!(
        canonical.functions[0]
            .return_type
            .as_ref()
            .unwrap()
            .path
            .segments,
        ["T", "T"]
    );
    assert_eq!(
        canonical.records[1].fields[0].ty.arguments[0].path.segments,
        ["T"]
    );
    assert_eq!(
        canonical.variants[0].cases[0].payloads[0].arguments[0]
            .path
            .segments,
        ["T"]
    );
}
