use cgx2_syntax::{format_module, parse_module, ExprKind, StmtKind};

#[test]
fn record_round_trip_keeps_fields_and_source_spans() {
    let source = "record Point:\n    x: i64\n    active: Bool\n\nfn main() -> i64:\n    let p = Point(active: true, x: 3)\n    p.x\n";
    let module = parse_module(source).unwrap();
    assert_eq!(module.records.len(), 1);
    assert_eq!(module.records[0].fields[0].name, "x");
    assert_eq!(
        &source[module.records[0].fields[0].ty.span.start..module.records[0].fields[0].ty.span.end],
        "i64"
    );
    let StmtKind::Expr(expr) = &module.functions[0].body[1].kind else {
        panic!("field expression")
    };
    let ExprKind::Name(path) = &expr.kind else {
        panic!("name path")
    };
    assert_eq!(path.segments, ["p", "x"]);
    assert_eq!(&source[path.span.start..path.span.end], "p.x");
    assert_eq!(format_module(&module), source);
    assert_eq!(parse_module(&format_module(&module)).unwrap(), module);
}

#[test]
fn record_field_count_indent_and_duplicate_are_diagnosed() {
    let only = "record Flag:\n    ok: Bool\n";
    assert_eq!(format_module(&parse_module(only).unwrap()), only);
    for (source, code) in [
        ("record Empty:\n    \n", "CGS104"),
        ("record Point:\n  x: i64\n", "CGS104"),
        ("record Point:\n    x: i64\n    x: Bool\n", "CGS112"),
    ] {
        assert!(
            parse_module(source)
                .unwrap_err()
                .iter()
                .any(|d| d.code == code),
            "{source}"
        );
    }
    let mut many = String::from("record Many:\n");
    for index in 0..9 {
        many.push_str(&format!("    f{index}: i64\n"));
    }
    assert!(parse_module(&many)
        .unwrap_err()
        .iter()
        .any(|d| d.code == "CGS113"));
}

#[test]
fn public_unicode_records_format_and_keep_utf8_spans() {
    let source = "pub record Pöint:\n    self: i64\n    live: Bool\n";
    let module = parse_module(source).unwrap();
    assert!(module.records[0].public);
    let span = module.records[0].span;
    assert!(source[span.start..span.end].starts_with("pub record Pöint:"));
    assert_eq!(format_module(&module), source);
    assert_eq!(parse_module(&format_module(&module)).unwrap(), module);
}
