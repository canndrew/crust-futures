use std::net::SocketAddr;
use tokio_core::net::TcpStream;
use tokio_core::reactor::Handle;
use tokio_io::IoFuture;
use futures::future;

use nat::util;

pub fn get_ext_addr(
    handle: &Handle,
    local_addr: &SocketAddr,
    peer_addr: &SocketAddr,
) -> IoFuture<SocketAddr> {
    let try = || {
        let query_socket = util::new_reusably_bound_tcp_socket(local_addr)?;
        let query_socket = query_socket.to_tcp_stream()?;
        let socket = TcpStream::connect_stream(query_socket, peer_addr, handle);

    };
    future::result(try()).boxed()
}

