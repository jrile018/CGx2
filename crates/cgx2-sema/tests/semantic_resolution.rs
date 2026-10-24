use cgx2_sema::{
    check_module, check_module_with_imports, collect_public_signatures, lower_module,
    ImportedSignatures, Type,
};
use cgx2_syntax::{parse_module, ExprKind, StmtKind};

const DATA_NAMES: &[&str] = &[
    "length",
    "show",
    "parse_i64",
    "array",
    "list",
    "get",
    "append",
    "concat",
    "updated",
    "slice",
];

#[test]
fn every_data_name_can_still_construct_a_record() {
    for name in DATA_NAMES {
        for prefix in ["", "module model\n"] {
            for generic in [false, true] {
                let declaration = if generic { "<T>" } else { "" };
                let field = if generic { "T" } else { "i64" };
                let application = if generic { "<i64>" } else { "" };
                let source = format!("{prefix}record {name}{declaration}:\n    value: {field}\nfn main() -> i64:\n    let x = {name}{application}(value: 42)\n    x.value\n");
                let module = parse_module(&source).unwrap();
                let typed = check_module(&module).unwrap();
                let StmtKind::Let { value, .. } = &module.functions[0].body[0].kind else {
                    panic!("expected binding")
                };
                let identity = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("model.{name}")
                };
                let expected = if generic {
                    Type::AppliedRecord {
                        name: identity,
                        arguments: vec![Type::I64],
                    }
                } else {
                    Type::Record(identity)
                };
                assert_eq!(typed.bodies["main"].get(value.span), Some(&expected));
                let lowered = lower_module(&module, &typed).unwrap();
                check_module(&lowered).unwrap();
                let StmtKind::Let { value, .. } = &lowered.functions[0].body[0].kind else {
                    panic!("expected lowered binding")
                };
                let ExprKind::Call { callee, .. } = &value.kind else {
                    panic!("expected lowered record call")
                };
                let ExprKind::Name(path) = &callee.kind else {
                    panic!("expected nominal identity")
                };
                assert_eq!(path.segments.join("."), lowered.records[0].name);
            }
        }
    }
}

fn rejects_constructor(source: &str, head: &str) {
    let module = parse_module(source).unwrap();
    let errors = check_module(&module).unwrap_err();
    let error = errors
        .iter()
        .find(|e| e.code == "CGT002" && e.message.contains("cannot be used as a constructor"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(&source[error.span.start..error.span.end], head);
}

#[test]
fn function_binders_shadow_record_and_variant_constructor_heads() {
    rejects_constructor(
        include_str!("semantic_resolution/binder-constructor.cg"),
        "T",
    );
    rejects_constructor("variant T:\n    Value(i64)\nfn f<T>(x: T) -> i64:\n    let broken = T.Value(1)\n    42\nfn main() -> i64:\n    f(1)\n", "T.Value(1)");
    rejects_constructor("record T<U>:\n    value: U\nfn f<T>(x: T) -> i64:\n    let broken = T<i64>(value: 1)\n    42\n", "T");
}

#[test]
fn nested_symbolic_bodies_reject_even_without_a_concrete_instantiation() {
    rejects_constructor("record T:\n    value: i64\nfn f<T>(x: T) -> i64:\n    if true:\n        let broken = T(value: 1)\n    else:\n        let fine = 1\n    42\nfn wrapper<U>(x: U) -> i64:\n    f<U>(x)\n", "T");
}

#[test]
fn qualified_nominal_head_is_available_beside_a_binder() {
    let module = parse_module(include_str!("semantic_resolution/qualified-binder.cg")).unwrap();
    let typed = check_module(&module).unwrap();
    assert!(matches!(
        typed.functions["f"].parameters[0],
        Type::Parameter { .. }
    ));
    let lowered = lower_module(&module, &typed).unwrap();
    check_module(&lowered).unwrap();
    assert_eq!(lowered.records[0].name, "model.T");
}

#[test]
fn declaration_binders_do_not_shadow_their_own_nominal_head() {
    for source in [
        "record T<T>:\n    value: T\nfn main() -> i64:\n    let x = T(value: 42)\n    x.value\n",
        "variant T<T>:\n    Value(T)\nfn main() -> T<i64>:\n    T.Value(42)\n",
    ] {
        let module = parse_module(source).unwrap();
        let typed = check_module(&module).unwrap();
        check_module(&lower_module(&module, &typed).unwrap()).unwrap();
    }
}

#[test]
fn checked_import_evidence_rejects_a_binder_constructor_body() {
    let module = parse_module("module dep\nrecord T:\n    value: i64\npub fn f<T>(x: T) -> i64 effects() caps():\n    let broken = T(value: 1)\n    42\n").unwrap();
    let mut imports = ImportedSignatures::default();
    imports.modules.insert("dep".into());
    for (name, signature) in collect_public_signatures(&module).unwrap() {
        imports
            .public_functions
            .insert(format!("dep.{name}"), signature);
    }
    assert!(imports
        .check_and_record_module(&module)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "CGT002"));
    let caller =
        parse_module("import dep\nfn main() -> i64 effects() caps():\n    dep.f(1)\n").unwrap();
    assert!(check_module_with_imports(&caller, &imports).is_err());
}

#[test]
fn qualified_imported_intrinsic_names_keep_nominal_identity() {
    for name in DATA_NAMES {
        let dependency =
            parse_module(&format!("module dep\npub record {name}:\n    value: i64\n")).unwrap();
        let mut imports = ImportedSignatures::default();
        imports.modules.insert("dep".into());
        let mut record = dependency.records[0].clone();
        record.name = format!("dep.{name}");
        imports.public_records.insert(record.name.clone(), record);
        let module = parse_module(&format!(
            "import dep\nfn main() -> i64:\n    let x = dep.{name}(value: 42)\n    x.value\n"
        ))
        .unwrap();
        check_module_with_imports(&module, &imports).unwrap();
        let unavailable = parse_module(&format!(
            "fn main() -> dep.{name}:\n    dep.{name}(value: 42)\n"
        ))
        .unwrap();
        assert!(check_module_with_imports(&unavailable, &imports)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "CGT008"));
    }
}

#[test]
fn lowering_does_not_turn_record_returning_functions_into_constructors() {
    let module = parse_module("record Box:\n    value: i64\nfn make() -> Box:\n    Box(value: 42)\nfn main() -> i64:\n    let x = make()\n    x.value\n").unwrap();
    let typed = check_module(&module).unwrap();
    let lowered = lower_module(&module, &typed).unwrap();
    check_module(&lowered).unwrap();
    let main = lowered.functions.iter().find(|f| f.name == "main").unwrap();
    let StmtKind::Let { value, .. } = &main.body[0].kind else {
        panic!("binding")
    };
    let ExprKind::Call { callee, .. } = &value.kind else {
        panic!("call")
    };
    assert!(matches!(&callee.kind, ExprKind::Name(path) if path.segments == ["make"]));
}
