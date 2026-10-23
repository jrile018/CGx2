use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

#[test]
fn range_loop_keeps_local_mutation_effect_and_checks_bounds() {
    let source = "fn twice(x: i64) -> i64:\n    x + x\nfn main() -> i64:\n    let mut sum = 0\n    for i in range(-2, 2):\n        sum += twice(i)\n    sum\n";
    let checked = check_module(&parse_module(source).unwrap()).unwrap();
    let effects = &checked.effects["main"].effects;
    assert!(effects.contains("Mutate"));
    assert!(!effects.contains("Unknown"));
    assert!(!effects.contains("Foreign"));

    for (source, code, needle) in [
        ("for i in range(1):\n    i\n", "CGT005", "range(1)"),
        ("for i in range(true, 2):\n    i\n", "CGT003", "true"),
        ("range(0, 2)\n", "CGT003", "range(0, 2)"),
        (
            "for i in range(0, 1):\n    return i\n",
            "CGT003",
            "return i",
        ),
        (
            "for i in range(0, 1):\n    arg_count()\n",
            "CGT012",
            "arg_count()",
        ),
    ] {
        let errors = check_module(&parse_module(source).unwrap()).unwrap_err();
        assert!(
            errors.iter().any(|error| error.code == code
                && source[error.span.start..error.span.end].contains(needle)),
            "{source}: {errors:?}"
        );
    }
}
