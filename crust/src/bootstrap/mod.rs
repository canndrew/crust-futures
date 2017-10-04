use std::io;
use std::net::SocketAddr;
use std::rc::Rc;
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
use peer::{Peer, HandshakeError};

mod cache;

use self::cache::Cache;

quick_error! {
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
        AllPeersFailed(e: Vec<TryPeerError>) {
            description("Failed to connect to any bootstrap peer")
            display("Failed to connect to any bootstrap peer ({} attempts failed)", e.len())
        }
    }
}

quick_error! {
    #[derive(Debug)]
    pub enum TryPeerError {
        Io(e: io::Error) {
            description("IO error connecting to remote peer")
            display("IO error connecting to remote peer: {}", e)
            from(e)
        }
        Handshake(e: HandshakeError) {
            description("Error during peer handshake")
            display("Error during peer handshake: {}", e)
            from()
        }
    }
}

pub fn bootstrap<UID: Uid>(
    our_uid: UID,
    name_hash: NameHash,
    ext_reachability: ExternalReachability,
    handle: &Handle,
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
            .infallible::<TryPeerError>()
            .map(|(_, v)| stream::iter_ok(v))
            .flatten();

        Ok(stream::iter_ok(peers)
            .chain(sd_peers)
            .map(move |addr| {
                try_peer(&handle, &addr, our_uid, name_hash, ext_reachability.clone())
            })
            .buffer_unordered(8)
            .first_ok()
            .map_err(BootstrapError::AllPeersFailed)
        )
    };
    future::result(try()).flatten().into_boxed()
}

fn try_peer<UID: Uid>(
    handle: &Handle,
    addr: &SocketAddr,
    our_uid: UID,
    name_hash: NameHash,
    ext_reachability: ExternalReachability,
) -> BoxFuture<Peer<UID>, TryPeerError> {
    let handle = handle.clone();
    TcpStream::connect(addr, &handle)
        .map_err(TryPeerError::Io)
        .and_then(move |stream| {
            let socket = Socket::wrap_tcp(&handle, stream);
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

