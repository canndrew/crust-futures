use std::io;
use std::net::SocketAddr;
use std::rc::Rc;
use std::collections::HashMap;
use tokio_core::reactor::Handle;
use tokio_core::net::TcpStream;
use futures::{future, stream, Future, Stream, Sink};
use future_utils::{FutureExt, StreamExt, BoxFuture};
use service_discovery;
use log::LogLevel;
use void::Void;
use config_file_handler;

use config::ConfigFile;
use common::{ExternalReachability, NameHash, Socket, SocketError, BootstrapDenyReason};
use uid::Uid;
use error::CrustError;
use service;
use common::SocketMessage;
use peer::{Peer, ConnectHandshakeError};

mod cache;

use self::cache::Cache;

quick_error! {
    /// Error returned when bootstrapping fails.
    #[derive(Debug)]
    pub enum BootstrapError {
        ReadCache(e: config_file_handler::Error)  {
            description("Error reading bootstrap cache")
            display("Error reading bootstrap cache: {}", e)
            from()
        }
        ServiceDiscovery(e: io::Error) {
            description("IO error using service discovery")
            display("IO error using service discovery: {}", e)
        }
        AllPeersFailed(e: HashMap<SocketAddr, TryPeerError>) {
            description("Failed to connect to any bootstrap peer")
            display("Failed to connect to any bootstrap peer, all {} attempts failed. Errors: {:?}", e.len(), e)
        }
    }
}

quick_error! {
    /// Error returned when we fail to connect to some specific peer.
    #[derive(Debug)]
    pub enum TryPeerError {
        Connect(e: io::Error) {
            description("IO error connecting to remote peer")
            display("IO error connecting to remote peer: {}", e)
            from(e)
        }
        Handshake(e: ConnectHandshakeError) {
            description("Error during peer handshake")
            display("Error during peer handshake: {}", e)
            from()
        }
    }
}

/// Try to bootstrap to the network.
///
/// On success, returns the peer that we've bootstrapped to.
pub fn bootstrap<UID: Uid>(
    handle: &Handle,
    our_uid: UID,
    name_hash: NameHash,
    ext_reachability: ExternalReachability,
    config: ConfigFile,
) -> BoxFuture<Peer<UID>, BootstrapError> {
    let handle = handle.clone();
    let try = || -> Result<_, BootstrapError> {
        let mut peers = Vec::new();
        let mut cache = Cache::new(config.read().bootstrap_cache_name.as_ref().map(|p| p.as_ref()))?;
        peers.extend(cache.read_file());
        peers.extend(config.read().hard_coded_contacts.iter().cloned());

        let sd_port = config.read().service_discovery_port
            .unwrap_or(service::SERVICE_DISCOVERY_DEFAULT_PORT);
        let sd_peers = service_discovery::discover::<Vec<SocketAddr>>(&handle, sd_port)
            .map_err(BootstrapError::ServiceDiscovery)?
            .infallible::<(SocketAddr, TryPeerError)>()
            .map(|(_, v)| stream::iter_ok(v))
            .flatten();

        Ok(stream::iter_ok(peers)
            .chain(sd_peers)
            .map(move |addr| {
                try_peer(&handle, &addr, our_uid, name_hash, ext_reachability.clone())
                .map_err(move |e| (addr, e))
            })
            .buffer_unordered(8)
            .first_ok()
            .map_err(|errs| BootstrapError::AllPeersFailed(errs.into_iter().collect()))
        )
    };
    future::result(try()).flatten().into_boxed()
}

/// Try to bootstrap to the given peer.
fn try_peer<UID: Uid>(
    handle: &Handle,
    addr: &SocketAddr,
    our_uid: UID,
    name_hash: NameHash,
    ext_reachability: ExternalReachability,
) -> BoxFuture<Peer<UID>, TryPeerError> {
    let handle = handle.clone();
    TcpStream::connect(addr, &handle)
        .and_then(move |stream| {
            Socket::wrap_tcp(&handle, stream)
        })
        .map_err(TryPeerError::Connect)
        .and_then(move |socket| {
            Peer::bootstrap_connect_handshake(
                socket,
                our_uid,
                name_hash,
                ext_reachability,
            )
            .map_err(TryPeerError::Handshake)
        })
        .into_boxed()
}

