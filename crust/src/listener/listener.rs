use std::marker::PhantomData;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use futures::{Async, Future, Stream};
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use future_utils::{self, BoxFuture, FutureExt, DropNotify, DropNotice};
use tokio_core::reactor::Handle;
use tokio_core::net::{TcpListener, Incoming};
use void;

use nat::{self, MappingContext, NatError};
use common::Socket;
use uid::Uid;

const LISTENER_BACKLOG: i32 = 100;

/// A handle for a single listening address. Drop this object to stop listening on this address.
pub struct Listener {
    _drop_tx: DropNotify,
    local_addr: SocketAddr,
}

/// A set of listeners.
pub struct Listeners<UID: Uid> {
    handle: Handle,
    listeners_tx: UnboundedSender<(DropNotice, Incoming, Vec<SocketAddr>)>,
    addresses: Arc<Mutex<Vec<SocketAddr>>>,
    _ph: PhantomData<UID>,
}

/// Created in tandem with a `Listeners`, represents the incoming stream of connections.
pub struct SocketIncoming<UID: Uid> {
    handle: Handle,
    listeners_rx: UnboundedReceiver<(DropNotice, Incoming, Vec<SocketAddr>)>,
    listeners: Vec<(DropNotice, Incoming, Vec<SocketAddr>)>,
    addresses: Arc<Mutex<Vec<SocketAddr>>>,
    _ph: PhantomData<UID>,
}

impl<UID: Uid> Listeners<UID> {
    /// Create an (empty) set of listeners and a handle to its incoming stream of connections.
    pub fn new(handle: &Handle) -> (Listeners<UID>, SocketIncoming<UID>) {
        let (tx, rx) = mpsc::unbounded();
        let addresses = Arc::new(Mutex::new(Vec::new()));
        let listeners = Listeners {
            handle: handle.clone(),
            listeners_tx: tx,
            addresses: addresses.clone(),
            _ph: PhantomData,
        };
        let incoming = SocketIncoming {
            handle: handle.clone(),
            listeners_rx: rx,
            listeners: Vec::new(),
            addresses: addresses,
            _ph: PhantomData,
        };
        (listeners, incoming)
    }

    /// All known addresses we may be contactable on. Includes global, NAT-mapped addresses.
    pub fn addresses(&self) -> Vec<SocketAddr> {
        unwrap!(self.addresses.lock()).clone()
    }

    /// Adds a new listener to the set of listeners, listening on the given local address, and
    /// returns a handle to it.
    pub fn listener(&self, listen_addr: &SocketAddr, mc: &MappingContext) -> BoxFuture<Listener, NatError> {
        let handle = self.handle.clone();
        let tx = self.listeners_tx.clone();
        nat::mapped_tcp_socket(mc, listen_addr)
        .and_then(move |(socket, addrs)| {
            let listener = socket.listen(LISTENER_BACKLOG)?;
            let local_addr = listener.local_addr()?;
            let listener = TcpListener::from_listener(listener, &local_addr, &handle)?;
            let incoming = listener.incoming();
            let (_drop_tx, drop_rx) = future_utils::drop_notify();
            let _ = tx.unbounded_send((drop_rx, incoming, addrs));
            Ok(Listener {
                _drop_tx,
                local_addr,
            })
        })
        .into_boxed()
    }
}

impl<UID: Uid> Stream for SocketIncoming<UID> {
    type Item = Socket<UID>;
    type Error = io::Error;

    fn poll(&mut self) -> io::Result<Async<Option<Socket<UID>>>> {
        while let Async::Ready(incoming_opt) = unwrap!(self.listeners_rx.poll()) {
            let (drop_rx, incoming, addrs) = match incoming_opt {
                Some(x) => x,
                None => return Ok(Async::Ready(None)),
            };
            self.listeners.push((drop_rx, incoming, addrs));
        }

        let mut i = 0;
        while i < self.listeners.len() {
            {
                let &mut (ref mut drop_notice, ref mut listener, _) = &mut self.listeners[i];
                if let Ok(Async::NotReady) = drop_notice.poll() {
                    if let Async::Ready(Some((stream, _addr))) = listener.poll()? {
                        if let Ok(socket) = Socket::wrap_tcp(&self.handle, stream) {
                            return Ok(Async::Ready(Some(socket)));
                        }
                    }
                    i += 1;
                    continue;
                }
            }
            self.listeners.swap_remove(i);
        }
        Ok(Async::NotReady)
    }
}

