use crate::local;
use crate::meter;
use crate::connection;
pub fn charge(units: i64) -> i64 {
    local::charge(units.checked_mul(2).expect("i64 arithmetic overflow"))
        .checked_add(meter::charge(units)).expect("i64 arithmetic overflow")
        .checked_add(connection::charge(units)).expect("i64 arithmetic overflow")

}
