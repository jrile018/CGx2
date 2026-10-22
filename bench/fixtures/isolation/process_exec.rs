fn main() {
    let result = std::process::Command::new("/usr/bin/id").output();
    if result.is_ok() { println!("escaped"); } else { println!("denied"); }
}
