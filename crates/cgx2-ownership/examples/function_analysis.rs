//! Manual S2 adapter input, not parsing or executing `.cg` borrowing.
use cgx2_ownership::v1::*;
use cgx2_syntax::Span;

const SOURCE: &str = include_str!("../fixtures/call-spans.cg");

fn lower_fixture(name: &str, returned: &str) -> Function {
    let start = SOURCE.find(&format!("fn {name}(")).unwrap();
    let tail = &SOURCE[start..];
    let end = tail.find("\n\n").map_or(SOURCE.len(), |i| start + i);
    let text = &SOURCE[start..end];
    let locate = |needle: &str| {
        let offset = text.rfind(needle).unwrap() + start;
        Span {
            start: offset,
            end: offset + needle.len(),
        }
    };
    Function {
        summary: FunctionSummary {
            name: name.into(),
            parameters: vec![ParameterMode::Owned],
            result: ResultMode::Owned {
                from_parameter: Some(0),
            },
            retention: Retention::Local,
        },
        locals: vec![
            Local {
                name: "owner".into(),
                storage: Storage::Owned,
                span: locate("owner"),
            },
            Local {
                name: "moved".into(),
                storage: Storage::Owned,
                span: locate("moved"),
            },
        ],
        blocks: vec![Block {
            operations: vec![SpannedOperation {
                operation: Operation::Move {
                    from: Place::local("owner"),
                    to: "moved".into(),
                },
                span: locate("let moved = owner"),
            }],
            terminator: Terminator::Return(ReturnValue::Owned(Place::local(returned))),
            span: locate(returned),
        }],
        entry: 0,
        span: Span { start, end },
    }
}

fn main() {
    let good = analyze_function(
        &lower_fixture("transfer", "moved"),
        &[],
        SOURCE,
        AnalysisLimits::default(),
    );
    assert!(good.is_accepted(), "{good:?}");
    println!("transfer: accepted typed ownership fixture");
    let bad = analyze_function(
        &lower_fixture("invalid_transfer", "owner"),
        &[],
        SOURCE,
        AnalysisLimits::default(),
    );
    assert!(!bad.is_accepted());
    let diagnostic = &bad.diagnostics[0];
    assert_eq!(diagnostic.code, "CGO101");
    assert_eq!(&SOURCE[diagnostic.span.start..diagnostic.span.end], "owner");
    println!(
        "invalid_transfer: {} at {}..{} ({})",
        diagnostic.code,
        diagnostic.span.start,
        diagnostic.span.end,
        &SOURCE[diagnostic.span.start..diagnostic.span.end]
    );
}
