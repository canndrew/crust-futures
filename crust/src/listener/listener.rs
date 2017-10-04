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

pub struct Listener {
    _drop_tx: DropNotify,
    local_addr: SocketAddr,
}

pub struct Listeners<UID: Uid> {
    handle: Handle,
    listeners_tx: UnboundedSender<(DropNotice, Incoming, Vec<SocketAddr>)>,
    addresses: Arc<Mutex<Vec<SocketAddr>>>,
    _ph: PhantomData<UID>,
}

pub struct SocketIncoming<UID: Uid> {
    handle: Handle,
    listeners_rx: UnboundedReceiver<(DropNotice, Incoming, Vec<SocketAddr>)>,
    listeners: Vec<(DropNotice, Incoming, Vec<SocketAddr>)>,
    addresses: Arc<Mutex<Vec<SocketAddr>>>,
    _ph: PhantomData<UID>,
}

impl<UID: Uid> Listeners<UID> {
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

    pub fn addresses(&self) -> Vec<SocketAddr> {
        unwrap!(self.addresses.lock()).clone()
    }

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
            tx.unbounded_send((drop_rx, incoming, addrs));
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
                    if let Async::Ready(Some((stream, addr))) = listener.poll()? {
                        let peer = Socket::wrap_tcp(&self.handle, stream);
                        return Ok(Async::Ready(Some(peer)));
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

