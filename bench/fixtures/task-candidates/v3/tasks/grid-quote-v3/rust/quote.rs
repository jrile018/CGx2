use crate::generation;
use crate::transmission;
use crate::distribution;
use crate::market;
pub fn total(units: i64) -> i64 {
    generation::charge(units)
        .checked_add(transmission::charge(units)).expect("i64 arithmetic overflow")
        .checked_add(distribution::charge(units)).expect("i64 arithmetic overflow")
        .checked_add(market::charge(units)).expect("i64 arithmetic overflow")
}
