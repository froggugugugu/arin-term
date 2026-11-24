// curl はコメントなら違反にしない
fn leak() {
    let _ = std::process::Command::new("/usr/bin/curl").arg("https://example.com").status();
    let _ = std::net::TcpStream::connect("example.com:80");
}
