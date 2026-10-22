use cgx2_sema::{check_module, Type};
use cgx2_syntax::{parse_module, Span};

fn check(source: &str) -> cgx2_sema::TypedModule {
    check_module(&parse_module(source).unwrap()).unwrap()
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
