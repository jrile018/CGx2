use cgx2_syntax::{
    format_module, lex, parse_module, Expr, ExprKind, Module, Span, Stmt, StmtKind, TokenKind,
    TypeSyntax,
};

fn kinds(source: &str) -> Vec<TokenKind> {
    lex(source).unwrap().into_iter().map(|t| t.kind).collect()
}

#[test]
fn lex_simple_function() {
    assert_eq!(
        kinds("pub fn double(x: i64) -> i64:\n    x * 2\n"),
        vec![
            TokenKind::Pub,
            TokenKind::Fn,
            TokenKind::Name("double".into()),
            TokenKind::LParen,
            TokenKind::Name("x".into()),
            TokenKind::Colon,
            TokenKind::Name("i64".into()),
            TokenKind::RParen,
            TokenKind::Arrow,
            TokenKind::Name("i64".into()),
            TokenKind::Colon,
            TokenKind::Newline,
            TokenKind::Indent,
            TokenKind::Name("x".into()),
            TokenKind::Star,
            TokenKind::Integer("2".into()),
            TokenKind::Newline,
            TokenKind::Dedent,
            TokenKind::Eof
        ]
    );
}

#[test]
fn lex_nested_indentation() {
    let ks = kinds("fn f():\n    for x in xs:\n        x\n    y\n");
    assert_eq!(ks.iter().filter(|k| **k == TokenKind::Indent).count(), 2);
    assert_eq!(ks.iter().filter(|k| **k == TokenKind::Dedent).count(), 2);
}

#[test]
fn reject_tab_indentation() {
    let errors = lex("fn f():\n\tx\n").unwrap_err();
    assert_eq!(errors[0].code, "CGS001");
    assert_eq!((errors[0].span.start, errors[0].span.end), (8, 9));
}

#[test]
fn reject_tab_on_continuation_line() {
    let errors = lex("let x = f(\n\t1)\n").unwrap_err();
    assert_eq!(errors[0].code, "CGS001");
    assert_eq!((errors[0].span.start, errors[0].span.end), (11, 12));
}

#[test]
fn retain_unicode_byte_spans() {
    let tokens = lex("let café = 1\r\n").unwrap();
    let name = tokens
        .iter()
        .find(|t| t.kind == TokenKind::Name("café".into()))
        .unwrap();
    assert_eq!((name.span.start, name.span.end), (4, 9));
    assert_eq!(
        tokens
            .iter()
            .find(|t| t.kind == TokenKind::Newline)
            .unwrap()
            .span
            .start,
        13
    );
}

#[test]
fn ignore_comment_only_indentation() {
    let ks = kinds("fn f():\n    x\n  # note\n    y\n");
    assert_eq!(ks.iter().filter(|k| **k == TokenKind::Indent).count(), 1);
    assert_eq!(ks.iter().filter(|k| **k == TokenKind::Dedent).count(), 1);
}

#[test]
fn parse_specification_functions() {
    let source = "module app.math\n\npub fn double(x: i64) -> i64:\n    x * 2\n\npub fn total(xs: List<i64>) -> i64:\n    let mut answer = 0\n    for x in xs:\n        answer += x\n    answer\n";
    let module = parse_module(source).unwrap();
    assert_eq!(module.name.as_ref().unwrap().segments, ["app", "math"]);
    assert_eq!(module.functions.len(), 2);
    assert!(module.functions[0].public);
    assert_eq!(module.functions[0].name, "double");
    assert_eq!(
        module.functions[1].parameters[0].ty.to_string(),
        "List<i64>"
    );
    assert!(matches!(
        module.functions[1].body[0].kind,
        StmtKind::Let { mutable: true, .. }
    ));
    assert!(matches!(
        module.functions[1].body[1].kind,
        StmtKind::For { .. }
    ));
}

#[test]
fn parse_top_level_script() {
    let module = parse_module("let x = 2\nprint(x + 1)\n").unwrap();
    assert_eq!(module.statements.len(), 2);
    assert!(module.functions.is_empty());
}

#[test]
fn reject_mixed_script_and_function_items() {
    let errors = parse_module("let x = 1\nfn f():\n    x\n").unwrap_err();
    assert_eq!(errors[0].code, "CGS105");
}

#[test]
fn parse_nested_for_blocks() {
    let module = parse_module("fn f(xs: List<i64>) -> i64:\n    for x in xs:\n        for y in xs:\n            x + y\n    0\n").unwrap();
    let StmtKind::For { body, .. } = &module.functions[0].body[0].kind else {
        panic!("outer for")
    };
    assert!(matches!(body[0].kind, StmtKind::For { .. }));
}

#[test]
fn parse_call_and_arithmetic_precedence() {
    let module = parse_module("let x = f(1 + 2 * 3)\n").unwrap();
    let StmtKind::Let { value, .. } = &module.statements[0].kind else {
        panic!("let")
    };
    let ExprKind::Call { arguments, .. } = &value.kind else {
        panic!("call")
    };
    let ExprKind::Binary { right, .. } = &arguments[0].value.kind else {
        panic!("sum")
    };
    assert!(matches!(right.kind, ExprKind::Binary { .. }));
}

#[test]
fn named_arguments_and_nested_guards_keep_byte_spans() {
    let source = "let café = memory.region(mode: manual, limit: 4096)\nno_gc(budget: 256):\n    no_gc(budget: 128):\n        memory.collect(café)\n";
    let module = parse_module(source).unwrap();
    let StmtKind::Let { value, .. } = &module.statements[0].kind else {
        panic!("let")
    };
    let ExprKind::Call { arguments, .. } = &value.kind else {
        panic!("call")
    };
    assert_eq!(arguments[0].name.as_deref(), Some("mode"));
    assert_eq!(
        &source[arguments[0].span.start..arguments[0].span.end],
        "mode: manual"
    );
    let StmtKind::NoGc { body, .. } = &module.statements[1].kind else {
        panic!("guard")
    };
    assert!(matches!(body[0].kind, StmtKind::NoGc { .. }));
    let once = format_module(&module);
    assert_eq!(once, format_module(&parse_module(&once).unwrap()));
}

#[test]
fn reject_duplicate_labels_and_positional_after_named() {
    let duplicate = parse_module("f(x: 1, x: 2)\n").unwrap_err();
    assert_eq!(duplicate[0].code, "CGS106");
    assert_eq!(duplicate[0].span, Span { start: 8, end: 9 });
    let mixed = parse_module("f(x: 1, 2)\n").unwrap_err();
    assert_eq!(mixed[0].code, "CGS107");
    assert_eq!(mixed[0].span, Span { start: 8, end: 9 });
    assert!(parse_module("f(1, x: 2)\n").is_ok());
    for source in [
        "no_gc():\n    1\n",
        "no_gc(limit: 1):\n    1\n",
        "no_gc(budget: 1, budget: 2):\n    1\n",
    ] {
        assert!(parse_module(source)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "CGS106"));
    }
    assert!(parse_module("no_gc(budget: -1):\n    1\n").is_err());
    assert_eq!(
        parse_module("no_gc(budget: 1):\n1\n").unwrap_err()[0].code,
        "CGS104"
    );
}

#[test]
fn missing_colon_has_source_diagnostic() {
    let errors = parse_module("fn f()\n    1\n").unwrap_err();
    assert_eq!(errors[0].code, "CGS101");
    assert_eq!(errors[0].span.start, 6);
}

#[test]
fn unexpected_dedent_has_source_diagnostic() {
    let errors = parse_module("fn f():\n    x\n  y\n").unwrap_err();
    assert_eq!(errors[0].code, "CGS002");
    assert_eq!((errors[0].span.start, errors[0].span.end), (14, 16));
}

const FIXTURES: &[&str] = &[
    "module app.math\n\npub fn double(x: i64) -> i64:\n    x * 2\n\npub fn total(xs: List<i64>) -> i64:\n    let mut answer = 0\n    for x in xs:\n        answer += x\n    answer\n",
    "let x = 2\nprint(x + 1)\n",
    "fn f(xs: List<i64>) -> i64:\n    for x in xs:\n        for y in xs:\n            x + y\n    0\n",
    "let x = f(1 + 2 * 3)\n",
    "let x = (1 + 2) * 3\n",
    "let x = 1 - (2 - 3)\n",
    "let scratch = memory.region(mode: manual, limit: 4096)\nno_gc(budget: 256):\n    no_gc(budget: 128):\n        memory.collect(scratch)\n",
];

#[test]
fn format_four_spaces_and_blank_lines() {
    let source =
        "module a.b\npub fn f(x:i64)->i64:\n  for y in xs:\n     x+y\npub fn g()->i64:\n  1";
    let module = parse_module(source).unwrap();
    assert_eq!(format_module(&module), "module a.b\n\npub fn f(x: i64) -> i64:\n    for y in xs:\n        x + y\n\npub fn g() -> i64:\n    1\n");
}

#[test]
fn formatting_is_idempotent() {
    for source in FIXTURES {
        let once = format_module(&parse_module(source).unwrap());
        let twice = format_module(&parse_module(&once).unwrap());
        assert_eq!(once, twice, "fixture: {source}");
        assert!(once.ends_with('\n'));
    }
}

#[test]
fn formatting_preserves_ast_structure() {
    for source in FIXTURES {
        let mut before = parse_module(source).unwrap();
        let mut after = parse_module(&format_module(&before)).unwrap();
        erase_spans(&mut before);
        erase_spans(&mut after);
        assert_eq!(before, after, "fixture: {source}");
    }
}

fn erase_type(ty: &mut TypeSyntax) {
    ty.span = Span { start: 0, end: 0 };
    ty.path.span = ty.span;
    for arg in &mut ty.arguments {
        erase_type(arg);
    }
}

fn erase_expr(expr: &mut Expr) {
    expr.span = Span { start: 0, end: 0 };
    match &mut expr.kind {
        ExprKind::Name(path) => path.span = expr.span,
        ExprKind::Binary { left, right, .. } => {
            erase_expr(left);
            erase_expr(right);
        }
        ExprKind::Call { callee, arguments } => {
            erase_expr(callee);
            for arg in arguments {
                arg.span = expr.span;
                erase_expr(&mut arg.value);
            }
        }
        _ => {}
    }
}

fn erase_stmts(stmts: &mut [Stmt]) {
    for stmt in stmts {
        stmt.span = Span { start: 0, end: 0 };
        match &mut stmt.kind {
            StmtKind::Let {
                annotation, value, ..
            } => {
                if let Some(ty) = annotation {
                    erase_type(ty);
                }
                erase_expr(value);
            }
            StmtKind::Assign { target, value, .. } => {
                target.span = stmt.span;
                erase_expr(value);
            }
            StmtKind::Return(value) => {
                if let Some(expr) = value {
                    erase_expr(expr);
                }
            }
            StmtKind::Expr(expr) => erase_expr(expr),
            StmtKind::For { iter, body, .. } => {
                erase_expr(iter);
                erase_stmts(body);
            }
            StmtKind::NoGc { budget, body } => {
                erase_expr(budget);
                erase_stmts(body);
            }
        }
    }
}

fn erase_spans(module: &mut Module) {
    module.span = Span { start: 0, end: 0 };
    if let Some(path) = &mut module.name {
        path.span = module.span;
    }
    erase_stmts(&mut module.statements);
    for function in &mut module.functions {
        function.span = module.span;
        for param in &mut function.parameters {
            param.span = module.span;
            erase_type(&mut param.ty);
        }
        if let Some(ty) = &mut function.return_type {
            erase_type(ty);
        }
        erase_stmts(&mut function.body);
    }
}
