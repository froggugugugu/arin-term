use std::os::unix::net::UnixStream;
use ::std::net::TcpStream;

pub fn open() -> (Option<UnixStream>, Option<TcpStream>) {
    (None, None)
}
