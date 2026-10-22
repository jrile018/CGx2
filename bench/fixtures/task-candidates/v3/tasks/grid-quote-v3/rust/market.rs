use crate::reserve;
use crate::balancing;
use crate::settlement;
pub fn charge(units: i64) -> i64 {
    reserve::charge(units.checked_mul(2).expect("i64 arithmetic overflow"))
        .checked_add(balancing::charge(units)).expect("i64 arithmetic overflow")
        .checked_add(settlement::charge(units)).expect("i64 arithmetic overflow")

}
