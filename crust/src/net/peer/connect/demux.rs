use std::sync::{Arc, Mutex};
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use log::LogLevel;

use net::listener::SocketIncoming;
use net::peer::connect::handshake_message::{HandshakeMessage, BootstrapRequest, ConnectRequest};
use net::peer::connect::BootstrapAcceptor;
use priv_prelude::*;

/// Demultiplexes the incoming stream of connections on the main listener and routes them to either
/// the bootstrap acceptor (if there is one), or to the appropriate connection handler.
pub struct Demux<UID: Uid> {
    inner: Arc<DemuxInner<UID>>,
}

struct DemuxInner<UID: Uid> {
    bootstrap_handler: Mutex<Option<UnboundedSender<(Socket<HandshakeMessage<UID>>, BootstrapRequest<UID>)>>>,
    connection_handler: Mutex<HashMap<UID, UnboundedSender<(Socket<HandshakeMessage<UID>>, ConnectRequest<UID>)>>>,
}

impl<UID: Uid> Demux<UID> {
    /// Create a demultiplexer from a stream of incoming peers.
    pub fn new(
        handle: &Handle,
        incoming: SocketIncoming,
    ) -> Demux<UID> {
        let inner = Arc::new(DemuxInner {
            bootstrap_handler: Mutex::new(None),
            connection_handler: Mutex::new(HashMap::new()),
        });
        let inner_cloned = inner.clone();
        let handle = handle.clone();
        let handler_task = {
            incoming
            .log_errors(LogLevel::Error, "listener errored!")
            .map(move |socket| {
                let socket = socket.change_message_type::<HandshakeMessage<UID>>();
                handle_incoming(inner_cloned.clone(), socket)
                .log_error(LogLevel::Debug, "handling incoming connection")
            })
            .buffer_unordered(128)
            .for_each(|()| Ok(()))
            .infallible()
        };
        handle.spawn(handler_task);
        Demux {
            inner: inner,
        }
    }

    pub fn bootstrap_acceptor(
        &self,
        handle: &Handle,
        config: ConfigFile,
        our_uid: UID,
    ) -> BootstrapAcceptor<UID> {
        let (acceptor, peer_tx) = BootstrapAcceptor::new(handle, config, our_uid);
        let mut bootstrap_handler = unwrap!(self.inner.bootstrap_handler.lock());
        *bootstrap_handler = Some(peer_tx);
        acceptor
    }
}

fn handle_incoming<UID: Uid>(
    inner: Arc<DemuxInner<UID>>,
    socket: Socket<HandshakeMessage<UID>>,
) -> BoxFuture<(), SocketError> {
    socket
    .into_future()
    .map_err(|(e, _s)| e)
    .map(move |(msg_opt, socket)| {
        let msg = match msg_opt {
            Some(msg) => msg,
            None => return,
        };
        match msg {
            HandshakeMessage::BootstrapRequest(bootstrap_request) => {
                let bootstrap_handler_opt = unwrap!(inner.bootstrap_handler.lock());
                if let Some(ref bootstrap_handler) = bootstrap_handler_opt.as_ref() {
                    let _ = bootstrap_handler.unbounded_send((socket, bootstrap_request));
                }
            },
            _ => (),
        }
    })
    .into_boxed()
}

