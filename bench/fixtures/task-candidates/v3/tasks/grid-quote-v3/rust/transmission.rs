use crate::line;
use crate::transformer;
use crate::switch;
pub fn charge(units: i64) -> i64 {
    line::charge(units.checked_mul(2).expect("i64 arithmetic overflow"))
        .checked_add(transformer::charge(units)).expect("i64 arithmetic overflow")
        .checked_add(switch::charge(units)).expect("i64 arithmetic overflow")

}
