use net::peer::PeerMessage;
use net::peer::connect::BootstrapDenyReason;
use util;
use priv_prelude::*;

pub struct Peer<UID: Uid> {
    their_uid: UID,
    socket: Socket<PeerMessage>,
}

/// A connection to a remote peer which is ready to start sending/receiving data on.
impl<UID: Uid> Peer<UID> {
    /// Construct a `Peer` from a `Socket` once we have completed the initial handshake.
    pub fn from_handshaken_socket<M: 'static>(
        socket: Socket<M>,
        their_uid: UID,
    ) -> Peer<UID> {
        Peer {
            socket: socket.change_message_type(),
            their_uid: their_uid,
        }
    }
}

