use std::io;
use std::net::IpAddr;
use futures::{future, stream, Future, Stream, Sink};
use future_utils::{BoxFuture, FutureExt, StreamExt};
use tokio_core::net::TcpStream;
use tokio_core::reactor::Handle;

use config::ConfigFile;
use uid::Uid;
use common::{CrustUser, NameHash, ExternalReachability, Socket, SocketError, BootstrapDenyReason, SocketMessage};
use util::ip_addr_is_global;

pub struct Peer<UID: Uid> {
    their_uid: UID,
    socket: Socket<UID>,
}

quick_error! {
    #[derive(Debug)]
    pub enum ConnectHandshakeError {
        Socket(e: SocketError) {
            description("Error on the underlying socket")
            display("Error on the underlying socket: {}", e)
            from()
        }
        BootstrapDenied(e: BootstrapDenyReason) {
            description("Bootstrap denied")
            display("Bootstrap denied. reason: {:?}", e)
            from(e)
        }
        InvalidResponse {
            description("Invalid response from peer")
        }
        Disconnected {
            description("Disconnected from peer")
        }
    }
}

quick_error! {
    #[derive(Debug)]
    pub enum AcceptHandshakeError {
        Socket(e: SocketError) {
            description("Error on the underlying socket")
            display("Error on the underlying socket: {}", e)
            from()
        }
        Disconnected {
            description("Disconnected from peer")
        }
        ConnectionFromOurself {
            description("Accepted connection from ourselves")
        }
        InvalidNameHash(name_hash: NameHash) {
            description("Peer is from a different network")
            display("Peer is from a different network. Invalid name hash == {:?}", name_hash)
        }
        NodeNotWhiteListed(ip: IpAddr) {
            description("Node is not whitelisted")
            display("Node {} is not whitelisted", ip)
        }
        FailedExternalReachability(errors: Vec<io::Error>) {
            description("All external reachability checks failed")
            display("All external reachability checks failed. Tried {} addresses, errors: {:?}", errors.len(), errors)
        }
        ClientNotWhiteListed(ip: IpAddr) {
            description("Client is not whitelisted")
            display("Client {} is not whitelisted", ip)
        }
    }
}

/// A connection to a remote peer which is ready to start sending/receiving data on.
impl<UID: Uid> Peer<UID> {
    /// Construct a `Peer` by performing a bootstrap connection handshake on a socket.
    pub fn bootstrap_connect_handshake(
        socket: Socket<UID>,
        our_uid: UID,
        name_hash: NameHash,
        ext_reachability: ExternalReachability,
    ) -> BoxFuture<Peer<UID>, ConnectHandshakeError> {
        let request = SocketMessage::BootstrapRequest(our_uid, name_hash, ext_reachability);
        socket.send((0, request))
        .map_err(ConnectHandshakeError::Socket)
        .and_then(|socket| {
            socket
                .into_future()
                .map_err(|(e, _)| ConnectHandshakeError::Socket(e))
                .and_then(|(msg_opt, socket)| {
                    match msg_opt {
                        Some(SocketMessage::BootstrapGranted(peer_uid)) => {
                            let peer = Peer {
                                their_uid: peer_uid,
                                socket: socket,
                            };
                            Ok(peer)
                        }
                        Some(SocketMessage::BootstrapDenied(reason)) => {
                            Err(ConnectHandshakeError::BootstrapDenied(reason))
                        }
                        Some(..) => {
                            Err(ConnectHandshakeError::InvalidResponse)
                        }
                        None => {
                            Err(ConnectHandshakeError::Disconnected)
                        }
                    }
                })
        })
        .into_boxed()
    }

    /// Construct a `Peer` by finishing a bootstrap accept handshake on a socket.
    /// The initial `BootstrapRequest` message sent by the peer has already been read from the
    /// socket.
    pub fn bootstrap_accept(
        handle: &Handle,
        socket: Socket<UID>,
        config: ConfigFile,
        our_uid: UID,
        their_uid: UID,
        their_name_hash: NameHash,
        their_ext_reachability: ExternalReachability,
    ) -> BoxFuture<Peer<UID>, AcceptHandshakeError>
    {
        let try = move || {
            if our_uid == their_uid {
                return Err(AcceptHandshakeError::ConnectionFromOurself);
            }
            if config.network_name_hash() != their_name_hash {
                return Ok(
                    socket.send((0, SocketMessage::BootstrapDenied(BootstrapDenyReason::InvalidNameHash)))
                    .map_err(AcceptHandshakeError::Socket)
                    .and_then(move |_socket| {
                        Err(AcceptHandshakeError::InvalidNameHash(their_name_hash))
                    })
                    .into_boxed()
                );
            }
            // Cache the reachability requirement config option, to make sure that it won't be
            // updated with the rest of the configuration.
            // TODO: why?
            let require_reachability = config.read().dev.as_ref().map_or(true, |dev_cfg| {
                !dev_cfg.disable_external_reachability_requirement
            });
            match config.reload() {
                Ok(()) => (),
                Err(e) => debug!("Could not read Crust config file: {:?}", e),
            };
            match their_ext_reachability {
                ExternalReachability::Required { direct_listeners } => {
                    let their_ip = socket.peer_addr()?.ip();
                    if !config.is_peer_whitelisted(their_ip, CrustUser::Node) {
                        let reason = BootstrapDenyReason::NodeNotWhitelisted;
                        return Ok(
                            socket.send((0, SocketMessage::BootstrapDenied(reason)))
                            .map_err(AcceptHandshakeError::Socket)
                            .and_then(move |_socket| {
                                Err(AcceptHandshakeError::NodeNotWhiteListed(their_ip))
                            })
                            .into_boxed()
                        );
                    }

                    if !require_reachability {
                        return Ok(
                            Peer::grant_bootstrap(socket, our_uid, their_uid)
                            .map_err(AcceptHandshakeError::Socket)
                            .into_boxed()
                        );
                    }

                    let connectors = {
                        direct_listeners
                        .into_iter()
                        .filter(|addr| ip_addr_is_global(&addr.ip()))
                        .map(|addr| TcpStream::connect(&addr, handle))
                        .collect::<Vec<_>>()
                    };
                    let connectors = stream::futures_unordered(connectors);

                    return Ok(
                        connectors
                        .first_ok()
                        .then(move |res| {
                            match res {
                                Ok(_connection) => {
                                    Peer::grant_bootstrap(socket, our_uid, their_uid)
                                    .map_err(AcceptHandshakeError::Socket)
                                    .into_boxed()
                                },
                                Err(v) => {
                                    let reason = BootstrapDenyReason::FailedExternalReachability;
                                    socket.send((0, SocketMessage::BootstrapDenied(reason)))
                                    .map_err(AcceptHandshakeError::Socket)
                                    .and_then(move |_socket| {
                                        Err(AcceptHandshakeError::FailedExternalReachability(v))
                                    })
                                    .into_boxed()
                                },
                            }
                        })
                        .into_boxed()
                    );
                },
                ExternalReachability::NotRequired => {
                    let their_ip = socket.peer_addr()?.ip();
                    if !config.is_peer_whitelisted(their_ip, CrustUser::Client) {
                        let reason = BootstrapDenyReason::ClientNotWhitelisted;
                        return Ok(
                            socket.send((0, SocketMessage::BootstrapDenied(reason)))
                            .map_err(AcceptHandshakeError::Socket)
                            .and_then(move |_socket| {
                                Err(AcceptHandshakeError::ClientNotWhiteListed(their_ip))
                            })
                            .into_boxed()
                        );
                    }

                    Ok(
                        Peer::grant_bootstrap(socket, our_uid, their_uid)
                        .map_err(AcceptHandshakeError::Socket)
                        .into_boxed()
                    )
                },
            }
        };
        future::result(try())
        .flatten()
        .into_boxed()
    }

    fn grant_bootstrap(
        socket: Socket<UID>,
        our_uid: UID,
        their_uid: UID,
    ) -> BoxFuture<Peer<UID>, SocketError> {
        socket.send((0, SocketMessage::BootstrapGranted(our_uid)))
        .map(move |socket| {
            Peer {
                their_uid,
                socket,
            }
        })
        .into_boxed()
    }
}

