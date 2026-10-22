pub fn finalize(subtotal: i64, shipping: i64, credit: i64) -> i64 {
    subtotal.checked_add(shipping).and_then(|n| n.checked_add(credit)).expect("i64 arithmetic overflow")
}
