use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_core::reactor::Handle;
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use futures::{future, Async, Future, Stream, Sink};
use future_utils::{BoxFuture, FutureExt, StreamExt};
use log::LogLevel;
use void::{self, Void};

use config::ConfigFile;
use listener::listener::SocketIncoming;
use common::{CrustUser, SocketError, SocketMessage, BootstrapDenyReason, ExternalReachability, Socket};
use peer::Peer;
use uid::Uid;

/// Processes a stream of incoming connections, performs handshakes on them, and forwards the newly
/// connected peers to where else in the program they're needed.
pub struct Demux<UID: Uid> {
    inner: Arc<DemuxInner<UID>>,
}

struct DemuxInner<UID: Uid> {
    our_uid: UID,
    config: ConfigFile,
    bootstrap_handler: Mutex<Option<UnboundedSender<Peer<UID>>>>,
    connection_handler: Mutex<HashMap<UID, UnboundedSender<Peer<UID>>>>,
}

impl<UID: Uid> Demux<UID> {
    /// Create a demultiplexer from a stream of incoming peers.
    pub fn new(
        handle: &Handle,
        our_uid: UID,
        config: ConfigFile,
        incoming: SocketIncoming<UID>,
    ) -> Demux<UID> {
        let inner = Arc::new(DemuxInner {
            our_uid: our_uid,
            config: config,
            bootstrap_handler: Mutex::new(None),
            connection_handler: Mutex::new(HashMap::new()),
        });
        let inner_cloned = inner.clone();
        let handle = handle.clone();
        let handler_task = {
            let handle = handle.clone();
            incoming
            .log_errors(LogLevel::Error, "listener errored!")
            .map(move |socket| {
                handle_incoming(&handle, inner_cloned.clone(), socket)
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

    /// Tells the `Demux` to start accepting bootstrap connections and returns a handle to the
    /// stream of incoming bootstrap peers. There can only be at most one such stream, each newly
    /// created stream will replace any pre-existing ones. This stream can be dropped to stop
    /// accepting bootstrap connections.
    pub fn bootstrap_acceptor(&self) -> BootstrapAcceptor<UID> {
        let (tx, rx) = mpsc::unbounded();
        {
            let mut bootstrap_handler = unwrap!(self.inner.bootstrap_handler.lock());
            *bootstrap_handler = Some(tx);
        }
        BootstrapAcceptor {
            inner: self.inner.clone(),
            rx: rx,
        }
    }
}

/// A stream of incoming bootstrap connections.
pub struct BootstrapAcceptor<UID: Uid> {
    inner: Arc<DemuxInner<UID>>,
    rx: UnboundedReceiver<Peer<UID>>,
}

fn handle_incoming<UID: Uid>(
    handle: &Handle,
    inner: Arc<DemuxInner<UID>>,
    socket: Socket<UID>,
) -> BoxFuture<(), SocketError> {
    let handle = handle.clone();
    socket
    .into_future()
    .map_err(|(e, _s)| e)
    .and_then(move |(msg_opt, socket)| {
        let msg = match msg_opt {
            Some(msg) => msg,
            None => return future::ok(()).into_boxed(),
        };
        match msg {
            SocketMessage::BootstrapRequest(their_uid, name_hash, ext_reachability) => {
                // TODO: don't accept if we don't have a bootstrap handler set.
                Peer::bootstrap_accept(
                    &handle,
                    socket,
                    inner.config.clone(),
                    inner.our_uid,
                    their_uid,
                    name_hash,
                    ext_reachability,
                )
                .map(move |peer| {
                    let bootstrap_handler = unwrap!(inner.bootstrap_handler.lock());
                    if let Some(sender) = bootstrap_handler.as_ref() {
                        let _ = sender.unbounded_send(peer);
                    }
                })
                .log_error(LogLevel::Debug, "accepting bootstrap connection")
                .infallible()
                .into_boxed()
            },
            _ => future::ok(()).into_boxed()
        }
    })
    .into_boxed()
}

impl<UID: Uid> Stream for BootstrapAcceptor<UID> {
    type Item = Peer<UID>;
    type Error = Void;

    fn poll(&mut self) -> Result<Async<Option<Peer<UID>>>, Void> {
        self.rx.poll().map_err(|()| unreachable!())
    }
}

impl<UID: Uid> Drop for BootstrapAcceptor<UID> {
    fn drop(&mut self) {
        let mut bootstrap_handler = unwrap!(self.inner.bootstrap_handler.lock());
        loop {
            match self.rx.poll() {
                Ok(Async::Ready(Some(_peer))) => (),
                Ok(Async::Ready(None)) => {
                    // Our sender has hung-up. So we are not the sender in bootstrap_handler. Leave
                    // bootstrap_handler alone.
                    break;
                },
                Ok(Async::NotReady) => {
                    // Our sender is still alive. We must be the sender in bootstrap_handler.
                    *bootstrap_handler = None;
                },
                Err(()) => unreachable!(),
            }
        }
    }
}

