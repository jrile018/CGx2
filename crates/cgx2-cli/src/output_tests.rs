use super::*;
use cgx2_sema::Type;

const SPAN: Span = Span { start: 4, end: 9 };

#[test]
fn output_budget_counts_names_type_arguments_fields_and_nested_punctuation() {
    let values = [
        Value::Record {
            name: "R".repeat(OUTPUT_BYTE_LIMIT + 1),
            fields: vec![],
        },
        Value::Record {
            name: "R".into(),
            fields: vec![("f".repeat(OUTPUT_BYTE_LIMIT), Value::Unit)],
        },
        Value::Variant {
            ty: Type::Variant {
                name: "Reply".into(),
                arguments: vec![Type::Record("T".repeat(OUTPUT_BYTE_LIMIT))],
            },
            case_index: 0,
            case_name: "Item".into(),
            payloads: vec![],
        },
        Value::Sequence {
            ty: Type::Array(Box::new(Type::String)),
            values: vec![Value::String("x".repeat(OUTPUT_BYTE_LIMIT - 6))],
        },
    ];
    for value in values {
        let error = finalize_value(value, SPAN).unwrap_err();
        assert_eq!(error.code, "CGR010");
        assert_eq!(error.message, "output exceeds 1048576 bytes");
        assert_eq!(error.span, Some(SPAN));
    }
}

#[test]
fn output_byte_boundary_keeps_values_intact_and_does_not_charge_print_newline() {
    for bytes in [
        OUTPUT_BYTE_LIMIT - 1,
        OUTPUT_BYTE_LIMIT,
        OUTPUT_BYTE_LIMIT + 1,
    ] {
        let value = Value::String(format!(
            "{}{}",
            "🙂".repeat(bytes / 4),
            "x".repeat(bytes % 4)
        ));
        let result = check_output_bound(&value, SPAN);
        assert_eq!(result.is_ok(), bytes <= OUTPUT_BYTE_LIMIT);
        if bytes <= OUTPUT_BYTE_LIMIT {
            assert_eq!(finalize_value(value.clone(), SPAN).unwrap().value, value);
        }
    }
}

#[test]
fn finalization_retains_positive_finite_value_depth_node_and_string_bounds() {
    let list = |length| Value::Sequence {
        ty: Type::List(Box::new(Type::Unit)),
        values: vec![Value::Unit; length],
    };
    assert!(finalize_value(list(65_536), SPAN).is_ok());
    let mut deep = Value::Unit;
    for _ in 0..63 {
        deep = Value::Record {
            name: "R".into(),
            fields: vec![("x".into(), deep)],
        };
    }
    assert!(finalize_value(deep.clone(), SPAN).is_ok());
    let deep = Value::Record {
        name: "R".into(),
        fields: vec![("x".into(), deep)],
    };
    for value in [
        list(65_537),
        deep,
        Value::String("x".repeat(OUTPUT_BYTE_LIMIT + 1)),
    ] {
        let error = finalize_value(value, SPAN).unwrap_err();
        assert_eq!(error.code, "CGR010");
        assert_eq!(error.message, "finite value exceeds size/depth budget");
        assert_eq!(error.span, Some(SPAN));
    }
}
