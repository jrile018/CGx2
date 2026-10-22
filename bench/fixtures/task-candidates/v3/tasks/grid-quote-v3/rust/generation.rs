use crate::solar;
use crate::wind;
use crate::hydro;
use crate::gas;
use crate::coal;
pub fn charge(units: i64) -> i64 {
    solar::charge(units.checked_mul(2).expect("i64 arithmetic overflow"))
        .checked_add(wind::charge(units)).expect("i64 arithmetic overflow")
        .checked_add(hydro::charge(units)).expect("i64 arithmetic overflow")
        .checked_add(gas::charge(units)).expect("i64 arithmetic overflow")
        .checked_add(coal::charge(units)).expect("i64 arithmetic overflow")

}
