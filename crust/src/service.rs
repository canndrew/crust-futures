use futures::sync::mpsc::UnboundedReceiver;
use priv_prelude::*;

use net::{self, Acceptor, Listener, BootstrapAcceptor, ServiceDiscovery};
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
    
    pub fn config(&self) -> ConfigFile {
        self.config.clone()
    }

    pub fn bootstrap(
        &mut self,
        blacklist: HashSet<SocketAddr>,
        crust_user: CrustUser,
    ) -> BoxFuture<Peer<UID>, BootstrapError> {
        let (current_addrs, _) = self.acceptor.addresses();
        let ext_reachability = match crust_user {
            CrustUser::Node => {
                ExternalReachability::Required {
                    direct_listeners: current_addrs.into_iter().collect(),
                }
            }
            CrustUser::Client => ExternalReachability::NotRequired,
        };
        net::bootstrap(
            &self.handle,
            self.our_uid,
            self.config.network_name_hash(),
            ext_reachability,
            blacklist,
            self.config.clone()
        )
    }

    pub fn bootstrap_acceptor(&mut self) -> BootstrapAcceptor<UID> {
        self.acceptor.bootstrap_acceptor()
    }

    pub fn start_listener(&self) -> BoxFuture<Listener, CrustError> {
        let port = self.config.read().tcp_acceptor_port.unwrap_or(0);
        let addr = SocketAddr::new(ip!("0.0.0.0"), port);
        self.acceptor.listener(&addr, &self.mc)
            .map_err(CrustError::StartListener)
            .into_boxed()
    }

    pub fn prepare_connection_info(&self) -> BoxFuture<PrivConnectionInfo<UID>, CrustError> {
        let our_uid = self.our_uid;
        let (direct_addrs, _) = self.acceptor.addresses();
        nat::mapped_tcp_socket::<UID>(&self.handle, &self.mc, &addr!("0.0.0.0:0"))
        .map(move |(socket, hole_punch_addrs)| {
            PrivConnectionInfo {
                id: our_uid,
                for_direct: direct_addrs.into_iter().collect(),
                for_hole_punch: hole_punch_addrs.into_iter().collect(),
                hole_punch_socket: Some(socket),
            }
        })
        .map_err(CrustError::PrepareConnectionInfo)
        .into_boxed()
    }

    pub fn connect(
        &self,
        our_info: PrivConnectionInfo<UID>,
        their_info: PubConnectionInfo<UID>,
    ) -> BoxFuture<Peer<UID>, ConnectError> {
        self.acceptor.connect(
            self.config.network_name_hash(),
            our_info,
            their_info,
        )
    }

    pub fn start_service_discovery(&self) -> io::Result<ServiceDiscovery> {
        let (current_addrs, addrs_rx) = self.acceptor.addresses();
        ServiceDiscovery::new(
            &self.handle,
            self.config.clone(),
            current_addrs,
            addrs_rx,
        )
    }

    pub fn addresses(&self) -> (HashSet<SocketAddr>, UnboundedReceiver<HashSet<SocketAddr>>) {
        self.acceptor.addresses()
    }

    pub fn id(&self) -> UID {
        self.our_uid
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }
}

