use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

#[test]
fn foreign_signatures_and_utf8_call_spans_are_checked() {
    let source = "foreign python fn isqrt(x: i64) -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime \"local:cpython\" ownership copied\n\nfn main() -> i64 effects() caps(ForeignCall):\n    let café = 4\n    isqrt(café)\n";
    let errors = check_module(&parse_module(source).unwrap()).unwrap_err();
    let effect = errors.iter().find(|error| error.code == "CGT012").unwrap();
    assert_eq!(&source[effect.span.start..effect.span.end], "isqrt(café)");
}
