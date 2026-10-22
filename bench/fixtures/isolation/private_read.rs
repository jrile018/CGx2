fn main() {
    match std::fs::read_to_string("{PRIVATE}") {
        Ok(_) => println!("escaped"),
        Err(_) => println!("denied"),
    }
}
