use uid::Uid;
use common::{NameHash, ExternalReachability, Socket, SocketError, BootstrapDenyReason, SocketMessage};
use futures::{Future, Stream, Sink};
use future_utils::{BoxFuture, FutureExt};

pub struct Peer<UID: Uid> {
    their_uid: UID,
    socket: Socket<UID>,
}

quick_error! {
    #[derive(Debug)]
    pub enum HandshakeError {
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

impl<UID: Uid> Peer<UID> {
    pub fn bootstrap_connect_handshake(
        socket: Socket<UID>,
        our_uid: UID,
        name_hash: NameHash,
        ext_reachability: ExternalReachability,
    ) -> BoxFuture<Peer<UID>, HandshakeError> {
        let request = SocketMessage::BootstrapRequest(our_uid, name_hash, ext_reachability);
        socket.send((0, request))
        .map_err(HandshakeError::Socket)
        .and_then(|socket| {
            socket
                .into_future()
                .map_err(|(e, _)| HandshakeError::Socket(e))
                .and_then(|(msg_opt, socket)| {
                    match msg_opt {
                        Some(SocketMessage::BootstrapGranted(peer_uid)) => {
                            let peer = Peer {
                                their_uid: peer_uid,
                                socket: socket,
                            };
                            Ok(peer)
                        }
                        Some(SocketMessage::BootstrapDenied(reason)) => {
                            Err(HandshakeError::BootstrapDenied(reason))
                        }
                        Some(..) => {
                            Err(HandshakeError::InvalidResponse)
                        }
                        None => {
                            Err(HandshakeError::Disconnected)
                        }
                    }
                })
        })
        .into_boxed()
    }
}

