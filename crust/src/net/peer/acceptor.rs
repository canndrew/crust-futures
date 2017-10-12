use futures::sync::mpsc::UnboundedReceiver;
use net::listener::{Listeners, Listener};
use net::peer::BootstrapAcceptor;
use net::peer::connect::Demux;

use priv_prelude::*;

pub struct Acceptor<UID: Uid> {
    listeners: Listeners,
    demux: Demux<UID>,
    handle: Handle,
    our_uid: UID,
    config: ConfigFile,
}

impl<UID: Uid> Acceptor<UID> {
    pub fn new(
        handle: &Handle,
        our_uid: UID,
        config: ConfigFile,
    ) -> Acceptor<UID> {
        let (listeners, socket_incoming) = Listeners::new(handle);
        let demux = Demux::new(handle, socket_incoming);
        let handle = handle.clone();
        Acceptor {
            listeners,
            demux,
            handle,
            config,
            our_uid,
        }
    }

    pub fn addresses(&self) -> (HashSet<SocketAddr>, UnboundedReceiver<HashSet<SocketAddr>>) {
        self.listeners.addresses()
    }

    pub fn listener(
        &self,
        listen_addr: &SocketAddr,
        mc: &MappingContext,
    ) -> BoxFuture<Listener, NatError> {
        self.listeners.listener(listen_addr, mc)
    }

    pub fn bootstrap_acceptor(&self) -> BootstrapAcceptor<UID> {
        self.demux.bootstrap_acceptor(
            &self.handle,
            self.config.clone(),
            self.our_uid,
        )
    }

    pub fn connect(
        &self,
        name_hash: NameHash,
        our_info: PrivConnectionInfo<UID>,
        their_info: PubConnectionInfo<UID>,
    ) -> BoxFuture<Peer<UID>, ConnectError> {
        self.demux.connect(
            &self.handle,
            name_hash,
            our_info,
            their_info,
            self.config.clone(),
        )
    }
}

