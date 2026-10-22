fn main() {
    let text = std::env::args().nth(1).expect("missing argument");
    println!("{text}");
}
