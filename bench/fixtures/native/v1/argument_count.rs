fn adjust(n: i64) -> i64 {
    n.checked_mul(7).unwrap().checked_add(1).unwrap()
}

fn main() {
    let count = i64::try_from(std::env::args().skip(1).count()).unwrap();
    println!("{}", adjust(count));
}
