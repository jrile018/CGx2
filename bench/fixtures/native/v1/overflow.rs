fn main() {
    let left = std::hint::black_box(i64::MAX);
    match left.checked_add(1) {
        Some(value) => println!("{value}"),
        None => {
            eprintln!("i64 arithmetic overflow");
            std::process::exit(1);
        }
    }
}
