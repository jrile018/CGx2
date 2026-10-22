fn main() {
    let value = (7_i64.checked_add(5).unwrap())
        .checked_mul(9_i64.checked_sub(3).unwrap()).unwrap()
        .checked_add(84_i64.checked_div(4).unwrap()).unwrap();
    println!("{value}");
}
