fn affine(x: i64, scale: i64, shift: i64) -> i64 {
    x.checked_mul(scale).unwrap().checked_add(shift).unwrap()
}

fn main() {
    println!("{}", affine(affine(7, 3, 2), 4, 9));
}
