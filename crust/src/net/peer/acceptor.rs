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

    pub fn addresses(&self) -> Vec<SocketAddr> {
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
}

