fn main() {
    let mut bytes = Vec::<u8>::new();
    if bytes.try_reserve_exact(3 * 1024 * 1024 * 1024).is_err() {
        println!("denied");
    } else {
        println!("escaped");
    }
}
