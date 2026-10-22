pub fn quote(units: i64, unit_price: i64, shipping: i64, credit: i64) -> i64 {
    adjust::finalize(units.checked_mul(unit_price).expect("i64 arithmetic overflow"), shipping, credit)
}
use crate::adjust;
