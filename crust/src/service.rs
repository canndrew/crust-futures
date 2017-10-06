use std::marker::PhantomData;
use std::net::{SocketAddr, SocketAddrV4};
use tokio_core::reactor::Handle;
use futures::{future, Future};
use future_utils::{FutureExt, BoxFuture};

use config::ConfigFile;
use uid::Uid;
use error::CrustError;
use bootstrap::{self, BootstrapError};
use common::{CrustUser, ExternalReachability, NameHash};
use nat::{mapping_context, MappingContext};
use listener::{Listener, Listeners};
use peer::Peer;

pub const SERVICE_DISCOVERY_DEFAULT_PORT: u16 = 5484;

pub struct Service<UID: Uid> {
    handle: Handle,
    config: ConfigFile,
    our_uid: UID,
    mc: MappingContext,
    listeners: Listeners<UID>,
}

impl<UID: Uid> Service<UID> {
    pub fn new(handle: &Handle, our_uid: UID) -> BoxFuture<Service<UID>, CrustError> {
        let try = || -> Result<_, CrustError> {
            Ok(Service::with_config(handle, ConfigFile::open_default()?, our_uid))
        };
        future::result(try()).flatten().into_boxed()
    }

    pub fn with_config(
        handle: &Handle,
        config: ConfigFile,
        our_uid: UID,
    ) -> BoxFuture<Service<UID>, CrustError> {
        let handle = handle.clone();
        let options = mapping_context::Options {
            force_include_port: config.read().force_acceptor_port_in_ext_ep,
        };
        MappingContext::new(options)
        .map_err(|e| CrustError::NatError(e))
        .map(move |mc| {
            let (listeners, _incoming) = Listeners::new(&handle);
            Service {
                handle: handle,
                config: config,
                our_uid: our_uid,
                mc: mc,
                listeners: listeners,
            }
        })
        .into_boxed()
    }

    pub fn bootstrap(&mut self, crust_user: CrustUser) -> BoxFuture<Peer<UID>, BootstrapError> {
        let ext_reachability = match crust_user {
            CrustUser::Node => {
                ExternalReachability::Required {
                    direct_listeners: self.listeners.addresses(),
                }
            }
            CrustUser::Client => ExternalReachability::NotRequired,
        };
        bootstrap::bootstrap(
            &self.handle,
            self.our_uid,
            self.config.network_name_hash(),
            ext_reachability,
            self.config.clone()
        )
    }

    pub fn start_listener(&self) -> BoxFuture<Listener, CrustError> {
        let port = self.config.read().tcp_acceptor_port.unwrap_or(0);
        let addr = SocketAddr::new(ip!("0.0.0.0"), port);
        self.listeners.listener(&addr, &self.mc)
            .map_err(CrustError::NatError)
            .into_boxed()
    }
}

/*
impl Config {
    pub fn is_peer_hard_coded(&self, UID) -> bool;
}

impl Service {
    pub fn new(our_uid: UID) -> impl Future<Item=Service>;
    pub fn with_config(config: ConfigFile, our_uid: UID) -> impl Future<Item=Service>;

    pub fn accept_bootstrap(&self) -> impl Future<Item=BootstrapAcceptor>;

    pub fn service_discovery_listener(&self) -> impl Future<Item=ServiceDiscoveryListener>;

    pub fn config(&self) -> &ConfigFile;

    pub fn start_bootstrap(&mut self, ...) -> impl Future<BootstrapConnect>;

    pub fn start_listener(...) -> impl Future<Listener>;

    pub fn prepare_connection_info(...) -> impl Future<ConnectionInfo>;
}

impl Stream for BootstrapAcceptor {
    Item = Peer;
}


impl Peer {
    pub fn ip(&self) -> IpAddr;
}

impl Stream,Sink for Peer;

impl Listener {
    pub fn port(&self) -> u16;
}

impl OurConnectionInfo {
    pub fn connect(...) -> impl Future<Item=Peer>;
}
*/

