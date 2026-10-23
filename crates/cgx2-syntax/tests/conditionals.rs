use cgx2_syntax::{format_module, parse_module, StmtKind};

#[test]
fn nested_unicode_conditionals_round_trip() {
    let source = "fn 選ぶ(旗: Bool) -> i64:\n    if 旗:\n        if false:\n            return 0\n        else:\n            return 42\n    else:\n        return 7\n";
    let module = parse_module(source).unwrap();
    let StmtKind::If {
        condition,
        then_body,
        else_body,
    } = &module.functions[0].body[0].kind
    else {
        panic!()
    };
    assert_eq!(&source[condition.span.start..condition.span.end], "旗");
    assert_eq!(then_body.len(), 1);
    assert_eq!(else_body.len(), 1);
    let formatted = format_module(&module);
    assert_eq!(formatted, format_module(&parse_module(&formatted).unwrap()));
}

#[test]
fn malformed_branches_are_source_located_errors() {
    for source in [
        "if true:\n    1\n",
        "else:\n    1\n",
        "if true:\n    1\nelse:\n    2\nelse:\n    3\n",
        "if true:\nelse:\n    2\n",
        "if true:\n    1\nelse:\n",
        "if true:\n    1\n  else:\n    2\n",
        "if true:\n\t1\nelse:\n    2\n",
        "let 名 = 1\nif true:\n    1\nelif false:\n    2\n",
    ] {
        let errors = parse_module(source).unwrap_err();
        assert!(!errors.is_empty());
        for error in errors {
            assert!(
                source.get(error.span.start..error.span.end).is_some(),
                "{error:?}"
            );
        }
    }
}
