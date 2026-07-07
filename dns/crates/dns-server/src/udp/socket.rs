use std::net::SocketAddr;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

pub fn create_socket(addr: SocketAddr) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;

    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;

    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;

    UdpSocket::from_std(socket.into())
}
