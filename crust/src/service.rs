use priv_prelude::*;

use net::{self, Acceptor, Listener};
use net::nat::{self, mapping_context};

pub const SERVICE_DISCOVERY_DEFAULT_PORT: u16 = 5484;

pub struct Service<UID: Uid> {
    handle: Handle,
    config: ConfigFile,
    our_uid: UID,
    mc: MappingContext,
    acceptor: Acceptor<UID>,
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
            let acceptor = Acceptor::new(&handle, our_uid, config.clone());
            Service {
                handle,
                config,
                our_uid,
                mc,
                acceptor,
            }
        })
        .into_boxed()
    }

    pub fn bootstrap(&mut self, crust_user: CrustUser) -> BoxFuture<Peer<UID>, BootstrapError> {
        let ext_reachability = match crust_user {
            CrustUser::Node => {
                ExternalReachability::Required {
                    direct_listeners: self.acceptor.addresses(),
                }
            }
            CrustUser::Client => ExternalReachability::NotRequired,
        };
        net::bootstrap(
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
        self.acceptor.listener(&addr, &self.mc)
            .map_err(CrustError::NatError)
            .into_boxed()
    }

    pub fn prepare_connection_info(&self) -> BoxFuture<PrivConnectionInfo<UID>, CrustError> {
        let our_uid = self.our_uid;
        let direct_addrs = self.acceptor.addresses();
        nat::mapped_tcp_socket(&self.mc, &addr!("0.0.0.0:0"))
        .map(move |(socket, hole_punch_addrs)| {
            PrivConnectionInfo {
                id: our_uid,
                for_direct: direct_addrs,
                for_hole_punch: hole_punch_addrs,
                hole_punch_socket: Some(socket),
            }
        })
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

