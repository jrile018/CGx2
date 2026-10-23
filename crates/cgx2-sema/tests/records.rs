use cgx2_sema::{check_module, Type};
use cgx2_syntax::parse_module;

fn diagnostics(source: &str) -> Vec<cgx2_sema::TypeDiagnostic> {
    check_module(&parse_module(source).unwrap()).unwrap_err()
}

#[test]
fn nominal_fields_and_nested_function_results_type_check() {
    let source = "record Point:\n    x: i64\n    live: Bool\nfn id(p: Point) -> Point:\n    p\nfn main() -> i64:\n    let p = id(Point(live: true, x: -9223372036854775808))\n    p.x\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    assert_eq!(
        typed.functions["id"].return_type,
        Type::Record("Point".into())
    );
}

#[test]
fn record_diagnostics_have_source_spans() {
    let header = "record Point:\n    x: i64\n    live: Bool\nfn main() -> i64:\n";
    for (body, code, phrase) in [
        ("    Point(x: 1)\n", "CGT019", "missing field"),
        (
            "    Point(x: 1, live: true, extra: 3)\n",
            "CGT019",
            "unknown field",
        ),
        ("    Point(x: true, live: true)\n", "CGT003", "expected i64"),
        (
            "    let p = Point(x: 1, live: true)\n    p.nope\n",
            "CGT021",
            "no field",
        ),
        ("    let n = 1\n    n.x\n", "CGT021", "not a record"),
        (
            "    p.x\n    let p = Point(x: 1, live: true)\n",
            "CGT001",
            "unknown name",
        ),
        (
            "    let p = Point(x: 1, live: true)\n    p.x = 2\n",
            "CGT004",
            "immutable record field",
        ),
    ] {
        let source = format!("{header}{body}");
        let errors = diagnostics(&source);
        let error = errors
            .iter()
            .find(|d| d.code == code && d.message.contains(phrase))
            .unwrap();
        assert!(error.span.start < error.span.end);
        assert!(error.span.end <= source.len());
    }
    assert!(diagnostics("record Point:\n    x: String\n")
        .iter()
        .any(|d| d.code == "CGT018"));
    assert!(diagnostics("fn main() -> Missing:\n    1\n")
        .iter()
        .any(|d| d.code == "CGT002"));
    assert!(diagnostics("fn main() -> i64:\n    Missing(x: 1)\n")
        .iter()
        .any(|d| d.code == "CGT002" && d.message.contains("unknown record type")));
    assert!(diagnostics("record Point:\n    x: i64\nfn take(xs: List<Point>) -> Unit:\n    return\nfn main() -> Unit:\n    return\n")
        .iter()
        .any(|d| d.code == "CGT023"));
}

#[test]
fn local_field_and_qualified_module_call_coexist_and_ambiguity_is_reported() {
    use cgx2_sema::{check_module_with_imports, FunctionSignature, ImportedSignatures};
    use cgx2_syntax::Span;
    let source = "module app\nimport lib\nrecord Point:\n    x: i64\nfn main() -> i64:\n    let p = Point(x: 3)\n    lib.double(p.x)\n";
    let mut imports = ImportedSignatures::default();
    imports.modules.insert("lib".into());
    imports.public_functions.insert(
        "lib.double".into(),
        FunctionSignature {
            public: true,
            parameters: vec![Type::I64],
            return_type: Type::I64,
            span: Span { start: 0, end: 0 },
        },
    );
    assert!(check_module_with_imports(&parse_module(source).unwrap(), &imports).is_ok());
    let ambiguous = source.replace(
        "    lib.double(p.x)\n",
        "    let lib = Point(x: 3)\n    lib.double(p.x)\n",
    );
    let errors =
        check_module_with_imports(&parse_module(&ambiguous).unwrap(), &imports).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGT020" && d.message.contains("ambiguous")));
}

#[test]
fn public_record_crossing_is_rejected_for_package_imports() {
    let module = parse_module(
        "module app\nrecord Point:\n    x: i64\npub fn make() -> Point:\n    Point(x: 1)\n",
    )
    .unwrap();
    let errors = cgx2_sema::collect_public_signatures(&module).unwrap_err();
    assert!(errors.iter().any(|d| d.code == "CGT022"));
}
