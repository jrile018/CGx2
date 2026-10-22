fn main() {
    let mut total = 11_i64;
    total = total.checked_add(17_i64.checked_mul(3).unwrap()).unwrap();
    total = total.checked_add(8).unwrap();
    println!("{total}");
}
