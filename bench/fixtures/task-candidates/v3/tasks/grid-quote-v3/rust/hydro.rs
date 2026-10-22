pub fn charge(units: i64) -> i64 { units.checked_mul(5).expect("i64 arithmetic overflow") }
