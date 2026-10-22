pub fn charge(units: i64) -> i64 { units.checked_mul(3).expect("i64 arithmetic overflow") }
