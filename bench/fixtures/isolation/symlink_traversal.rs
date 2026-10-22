fn main() {
    let link = "/scratch/private-link";
    let result = std::os::unix::fs::symlink("{PRIVATE}", link)
        .and_then(|_| std::fs::read_to_string(link));
    if result.is_ok() { println!("escaped"); } else { println!("denied"); }
}
