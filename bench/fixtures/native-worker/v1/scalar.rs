fn main() {
    let mut x = std::env::args().skip(1).count() as i64 + 17;
    for i in 0..(20_000 + std::env::args().skip(1).count() as i64) {
        x = x.checked_mul(31).unwrap().checked_add(i).unwrap() % 1_000_000_007;
    }
    println!("{x}");
}
