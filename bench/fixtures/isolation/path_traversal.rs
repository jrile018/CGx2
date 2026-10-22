fn main() {
    let relative = format!("/workspace/../../{}", "{PRIVATE}".trim_start_matches('/'));
    if std::fs::read_to_string(relative).is_ok() {
        println!("escaped");
    } else {
        println!("denied");
    }
}
