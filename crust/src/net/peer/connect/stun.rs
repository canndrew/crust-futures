use priv_prelude::*;
use net::peer::connect::HandshakeMessage;

quick_error! {
    #[derive(Debug)]
    pub enum StunError {
        Connect(e: io::Error) {
            description("error connecting to peer")
            display("error connecting to peer: {}", e)
            cause(e)
        }
        UnexpectedResponse {
            description("the peer replied with an unexpected message")
        }
        Disconnected {
            description("disconnected from the remote peer")
        }
        Socket(e: SocketError) {
            description("error on the socket")
            display("error on the socket: {}", e)
            cause(e)
        }
    }
}

pub fn stun<UID: Uid>(
    handle: &Handle,
    peer_addr: &SocketAddr,
) -> BoxFuture<SocketAddr, StunError> {
    let handle = handle.clone();
    let peer_addr = *peer_addr;
    TcpStream::connect(&peer_addr, &handle)
    .map_err(|e| StunError::Connect(e))
    .map(move |stream| Socket::<HandshakeMessage<UID>>::wrap_tcp(&handle, stream, peer_addr))
    .and_then(|socket| {
        socket
        .send((0, HandshakeMessage::EchoAddrReq))
        .and_then(|socket| {
            socket
            .into_future()
            .map_err(|(e, _)| e)
        })
        .map_err(StunError::Socket)
        .and_then(|(msg_opt, _socket)| {
            match msg_opt {
                Some(HandshakeMessage::EchoAddrResp(addr)) => Ok(addr),
                Some(..) => Err(StunError::UnexpectedResponse),
                None => Err(StunError::Disconnected),
            }
        })
    })
    .into_boxed()
}

