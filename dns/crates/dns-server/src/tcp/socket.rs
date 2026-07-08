use std::net::SocketAddr;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;

pub fn create_socket(addr: SocketAddr, backlog: i32) -> std::io::Result<TcpListener> {
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;

    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;

    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(backlog)?;

    TcpListener::from_std(socket.into())
}
