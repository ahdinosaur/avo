use std::{net::TcpStream, time::Duration};

pub fn is_tcp_port_open(port: u16) -> bool {
    let mut buf = [0];
    TcpStream::connect(format!("127.0.0.1:{port}"))
        .and_then(|stream| {
            stream.set_read_timeout(Some(Duration::from_secs(1)))?;
            stream.peek(&mut buf)
        })
        .is_ok()
}
