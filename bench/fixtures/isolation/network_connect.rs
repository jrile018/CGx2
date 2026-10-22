fn main() {
    let result = std::net::TcpStream::connect("127.0.0.1:1");
    if result.is_ok() { println!("escaped"); } else { println!("denied"); }
}
