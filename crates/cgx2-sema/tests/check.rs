use cgx2_sema::{
    check_module, check_module_with_imports, collect_public_signatures, ImportedSignatures, Type,
};
use cgx2_syntax::{parse_module, Span};

#[test]
fn standalone_check_rejects_import_execution_at_import_span() {
    let source = "module app.main\nimport app.math\npub fn run() -> i64:\n    1\n";
    let module = parse_module(source).unwrap();
    let errors = check_module(&module).unwrap_err();
    assert_eq!(errors[0].code, "CGT011");
    assert_eq!(
        &source[errors[0].span.start..errors[0].span.end],
        "app.math"
    );
}

fn check(source: &str) -> cgx2_sema::TypedModule {
    check_module(&parse_module(source).unwrap()).unwrap()
}

#[test]
fn explicit_effects_and_capabilities_gate_direct_and_transitive_calls() {
    error(
        "fn f() -> String effects() caps():\n    arg(0)\n",
        "CGT012",
        "arg(0)",
    );
    error(
        "fn f() -> String effects(ReadArgs) caps():\n    arg(0)\n",
        "CGT013",
        "arg(0)",
    );
    error(
        "fn helper() -> i64:\n    arg_count()\nfn f() -> i64 effects() caps():\n    helper()\n",
        "CGT012",
        "helper()",
    );
    error(
        "fn helper() -> i64:\n    arg_count()\nfn f() -> i64 effects(ReadArgs) caps():\n    helper()\n",
        "CGT013",
        "helper()",
    );
    error(
        "fn f() -> i64 effects() caps():\n    missing()\n",
        "CGT012",
        "missing()",
    );
    error(
        "fn f() -> Unit effects() caps():\n    memory.collect(1)\n",
        "CGT012",
        "memory.collect(1)",
    );
    error(
        "fn f() -> Unit effects(Collect) caps():\n    memory.collect(1)\n",
        "CGT013",
        "memory.collect(1)",
    );
    let typed = check("fn f() -> i64 effects(ReadArgs) caps(ArgsRead):\n    arg_count()\n");
    assert!(typed.effects["f"].effects_explicit);
    assert!(typed.effects["f"].capabilities.contains("ArgsRead"));
    let dependency = parse_module("module dep\npub fn value() -> i64:\n    1\n").unwrap();
    let mut imports = ImportedSignatures::default();
    imports.modules.insert("dep".into());
    imports.public_functions.insert(
        "dep.value".into(),
        collect_public_signatures(&dependency)
            .unwrap()
            .remove("value")
            .unwrap(),
    );
    let source = "module app\nimport dep\nfn f() -> i64 effects() caps():\n    dep.value()\n";
    let errors = check_module_with_imports(&parse_module(source).unwrap(), &imports).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGT012" && &source[d.span.start..d.span.end] == "dep.value()"));
}

#[test]
fn contracts_require_pure_bool_scalar_expressions() {
    check("fn f(x: i64) -> i64 effects() caps():\n    requires x >= 0\n    ensures result >= x\n    x\n");
    error(
        "fn f() -> Bool:\n    requires arg_count() == 0\n    true\n",
        "CGT014",
        "arg_count() == 0",
    );
    error(
        "fn f(x: i64) -> i64:\n    requires x + 1\n    x\n",
        "CGT003",
        "x + 1",
    );
    error(
        "fn f() -> Bool:\n    requires f()\n    true\n",
        "CGT014",
        "f()",
    );
    error(
        "fn f(result: i64) -> i64:\n    ensures result == 1\n    result\n",
        "CGT014",
        "fn f(result: i64) -> i64:\n    ensures result == 1\n    result\n",
    );
}

fn error(source: &str, code: &str, text: &str) {
    let errors = check_module(&parse_module(source).unwrap()).unwrap_err();
    let found = errors
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(&source[found.span.start..found.span.end], text);
}

#[test]
fn scoped_names_and_type_table() {
    let source =
        "fn f(xs: List<i64>) -> i64:\n    let x = 1\n    for x in xs:\n        x + 2\n    x\n";
    let typed = check(source);
    assert_eq!(
        typed.functions["f"].parameters,
        vec![Type::List(Box::new(Type::I64))]
    );
    let p = source.rfind("    x\n").unwrap() + 4;
    assert_eq!(
        typed.expressions.get(Span {
            start: p,
            end: p + 1
        }),
        Some(&Type::I64)
    );
    error(
        "fn f(xs: List<i64>) -> i64:\n    for x in xs:\n        missing\n    0\n",
        "CGT001",
        "missing",
    );
    let unicode = "fn f(xs: List<i64>) -> i64:\n    for x in xs:\n        café\n    0\n";
    let errors = check_module(&parse_module(unicode).unwrap()).unwrap_err();
    assert_eq!(errors[0].span.start, unicode.find("café").unwrap());
    assert_eq!(&unicode[errors[0].span.start..errors[0].span.end], "café");
    error("fn f(x: i64, x: i64) -> i64:\n    x\n", "CGT001", "x");
    error("let x = 1\nlet x = 2\n", "CGT001", "x");
    error(
        "fn f() -> i64:\n    1\nfn f() -> i64:\n    2\n",
        "CGT001",
        "f",
    );
    error("fn f(x: Mystery) -> i64:\n    1\n", "CGT002", "Mystery");
    error(
        "fn f(x: List<i64, Bool>) -> i64:\n    1\n",
        "CGT002",
        "List<i64, Bool>",
    );
}

#[test]
fn expressions_and_mutation() {
    check("let x = 1 + 2 * 3\nlet y = true\nlet z = \"hello\"\nx\n");
    check("let mut x = 1\nx = 2\nx += 3\n");
    error("let x = 1\nx = 2\n", "CGT004", "x");
    error("let mut x = \"a\"\nx += \"b\"\n", "CGT003", "x += \"b\"\n");
    error(
        "let x = 9223372036854775808\n",
        "CGT007",
        "9223372036854775808",
    );
    error(
        "fn f(x: i64) -> i64:\n    for y in x:\n        y\n    0\n",
        "CGT003",
        "x",
    );
    error("let x = true + 1\n", "CGT003", "true + 1");
}

#[test]
fn calls_and_returns() {
    check("fn add(x: i64, y: i64) -> i64:\n    x + y\nfn call() -> i64:\n    add(1, 2)\n");
    check("pub fn f() -> i64:\n    arg_count()\n");
    check("fn f() -> String:\n    arg(0)\n");
    check("fn f() -> i64:\n    return 1\n");
    check("fn f():\n    1\n");
    let typed = check("fn first() -> i64:\n    later()\nfn later():\n    4\n");
    assert_eq!(typed.functions["later"].return_type, Type::I64);
    error("let x = missing()\n", "CGT001", "missing");
    error("let x = arg()\n", "CGT005", "arg()");
    error(
        "fn f() -> i64:\n    arg_count(1)\n",
        "CGT005",
        "arg_count(1)",
    );
    error("let x = arg(true)\n", "CGT003", "arg(true)");
    error("pub fn f():\n    1\n", "CGT006", "pub fn f():\n    1\n");
    error("pub fn f() -> Bool:\n    1\n", "CGT003", "1");
    error("fn f() -> Bool:\n    return 1\n", "CGT003", "1");
}
