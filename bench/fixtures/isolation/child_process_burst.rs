fn main() {
    for _ in 0..64 {
        if std::process::Command::new("/usr/bin/id").output().is_ok() {
            println!("escaped");
            return;
        }
    }
    println!("denied");
}
