pub fn get_free_tcp_port() -> Option<u16> {
    let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);
    let tcp = TcpListener::bind(addr).ok()?;
    let port = tcp.local_addr().ok()?.port();
    Some(port)
}
