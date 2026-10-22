fn main() {
    for fd in 3..64 {
        if std::fs::read_link(format!("/proc/self/fd/{fd}")).is_ok() {
            println!("escaped");
            return;
        }
    }
    println!("denied");
}
