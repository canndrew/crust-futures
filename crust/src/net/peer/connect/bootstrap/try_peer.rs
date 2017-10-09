use net::peer::connect::handshake_message::{HandshakeMessage, BootstrapDenyReason, BootstrapRequest};
use priv_prelude::*;

quick_error! {
    /// Error returned when we fail to connect to some specific peer.
    #[derive(Debug)]
    pub enum TryPeerError {
        Connect(e: io::Error) {
            description("IO error connecting to remote peer")
            display("IO error connecting to remote peer: {}", e)
            from(e)
        }
        Handshake(e: ConnectHandshakeError) {
            description("Error during peer handshake")
            display("Error during peer handshake: {}", e)
            from()
        }
    }
}

quick_error! {
    #[derive(Debug)]
    pub enum ConnectHandshakeError {
        Socket(e: SocketError) {
            description("Error on the underlying socket")
            display("Error on the underlying socket: {}", e)
            from()
        }
        BootstrapDenied(e: BootstrapDenyReason) {
            description("Bootstrap denied")
            display("Bootstrap denied. reason: {:?}", e)
            from(e)
        }
        InvalidResponse {
            description("Invalid response from peer")
        }
        Disconnected {
            description("Disconnected from peer")
        }
    }
}

/// Try to bootstrap to the given peer.
pub fn try_peer<UID: Uid>(
    handle: &Handle,
    addr: &SocketAddr,
    our_uid: UID,
    name_hash: NameHash,
    ext_reachability: ExternalReachability,
) -> BoxFuture<Peer<UID>, TryPeerError> {
    let handle = handle.clone();
    TcpStream::connect(addr, &handle)
        .and_then(move |stream| {
            Socket::wrap_tcp(&handle, stream)
        })
        .map_err(TryPeerError::Connect)
        .and_then(move |socket| {
            bootstrap_connect_handshake(
                socket,
                our_uid,
                name_hash,
                ext_reachability,
            )
            .map_err(TryPeerError::Handshake)
        })
        .into_boxed()
}

/// Construct a `Peer` by performing a bootstrap connection handshake on a socket.
pub fn bootstrap_connect_handshake<UID: Uid>(
    socket: Socket<HandshakeMessage<UID>>,
    our_uid: UID,
    name_hash: NameHash,
    ext_reachability: ExternalReachability,
) -> BoxFuture<Peer<UID>, ConnectHandshakeError> {
    let request = BootstrapRequest {
        uid: our_uid,
        name_hash: name_hash,
        ext_reachability: ext_reachability,
    };
    let msg =  HandshakeMessage::BootstrapRequest(request);
    socket.send((0, msg))
    .map_err(ConnectHandshakeError::Socket)
    .and_then(|socket| {
        socket
            .into_future()
            .map_err(|(e, _)| ConnectHandshakeError::Socket(e))
            .and_then(|(msg_opt, socket)| {
                match msg_opt {
                    Some(HandshakeMessage::BootstrapGranted(peer_uid)) => {
                        let peer = Peer::from_handshaken_socket(socket, peer_uid);
                        Ok(peer)
                    }
                    Some(HandshakeMessage::BootstrapDenied(reason)) => {
                        Err(ConnectHandshakeError::BootstrapDenied(reason))
                    }
                    Some(..) => {
                        Err(ConnectHandshakeError::InvalidResponse)
                    }
                    None => {
                        Err(ConnectHandshakeError::Disconnected)
                    }
                }
            })
    })
    .into_boxed()
}

