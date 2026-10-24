use cgx2_syntax::{format_module, parse_module};
#[test]
fn generic_calls_collections_and_patterns_round_trip() {
    let source="record Box<T: Eq + Show>:\n    value: T\nvariant V<T>:\n    Some(T)\n    None\nfn f<T: Eq + Show>(x: T) -> T:\n    x\nfn main() -> i64:\n    let xs: List<Array<i64>> = [[1]]\n    let box = Box<i64>(value: f<i64>(1))\n    let x = V<i64>.Some(box.value)\n    match x:\n        V<i64>.Some(value):\n            value\n        V<i64>.None:\n            0\n";
    let first = parse_module(source).unwrap();
    let rendered = format_module(&first);
    let second = parse_module(&rendered).unwrap();
    assert_eq!(format_module(&second), rendered);
    assert!(rendered.contains("f<i64>(1)"));
    assert!(rendered.contains("Box<i64>(value:"));
}
