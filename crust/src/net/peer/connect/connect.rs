use futures::sync::mpsc::UnboundedReceiver;
use net::peer;
use net::peer::connect::handshake_message::{HandshakeMessage, ConnectRequest};
use net::nat;
use priv_prelude::*;

const TIMEOUT_SEC: u64 = 60;

quick_error! {
    #[derive(Debug)]
    pub enum ConnectError {
        RequestedConnectToSelf {
            description("requested a connection to ourselves")
        }
        Io(e: io::Error) {
            description("io error initiating connection")
            display("io error initiating connection: {}", e)
            cause(e)
        }
        ChooseConnection(e: SocketError) {
            description("socket error when finalising handshake")
            display("socket error when finalising handshake: {}", e)
            cause(e)
        }
        AllConnectionsFailed(v: Vec<SingleConnectionError>) {
            description("all attempts to connect to the remote peer failed")
            display("all {} attempts to connect to the remote peer failed: {:?}", v.len(), v)
        }
        TimedOut {
            description("connection attempt timed out")
        }
    }
}

quick_error! {
    #[derive(Debug)]
    pub enum SingleConnectionError {
        Io(e: io::Error) {
            description("io error initiating/accepting connection")
            display("io error initiating/accepting connection: {}", e)
            cause(e)
        }
        Socket(e: SocketError) {
            description("io error socket error")
            display("io error on socket: {}", e)
            cause(e)
        }
        ConnectionDropped {
            description("the connection was dropped by the remote peer")
        }
        InvalidUid(formatted_uid: String) {
            description("Peer gave us an unexpected uid")
            display("Peer gave us an unexpected uid: {}", formatted_uid)
        }
        InvalidNameHash(name_hash: NameHash) {
            description("Peer is from a different network")
            display("Peer is from a different network. Invalid name hash == {:?}", name_hash)
        }
        UnexpectedMessage {
            description("Peer sent us an unexpected message variant")
        }
    }
}

pub fn connect<UID: Uid>(
    handle: &Handle,
    name_hash: NameHash,
    our_info: PrivConnectionInfo<UID>,
    their_info: PubConnectionInfo<UID>,
    config: ConfigFile,
    peer_rx: UnboundedReceiver<(Socket<HandshakeMessage<UID>>, ConnectRequest<UID>)>,
) -> BoxFuture<Peer<UID>, ConnectError> {
    let try = move || {
        let handle = handle.clone();
        let their_info = their_info;
        let our_uid = our_info.id;
        let their_uid = their_info.id;

        if our_uid == their_uid {
            return Err(ConnectError::RequestedConnectToSelf);
        }

        let our_connect_request = ConnectRequest {
            uid: our_uid,
            name_hash: name_hash,
        };

        let direct_incoming = {
            let our_connect_request = our_connect_request.clone();
            peer_rx
            .map_err(|()| unreachable!())
            .infallible::<SingleConnectionError>()
            .and_then(move |(socket, connect_request)| {
                let their_uid = validate_connect_request(our_uid, name_hash, connect_request)?;
                Ok({
                    socket
                    .send((0, HandshakeMessage::Connect(our_connect_request.clone())))
                    .map_err(SingleConnectionError::Socket)
                    .map(move |socket| (socket, their_uid))
                })
            })
            .buffer_unordered(1)
        };

        let mut their_direct = their_info.for_direct;
        let mut their_hole_punch = their_info.for_hole_punch;
        if let Some(ref whitelisted_node_ips) = config.read().whitelisted_node_ips {
            their_direct.retain(|s| whitelisted_node_ips.contains(&s.ip()));
            their_hole_punch.retain(|s| whitelisted_node_ips.contains(&s.ip()));
        }

        let other_connections = {
            let handle = handle.clone();
            let direct_connections = {
                let connectors = {
                    their_direct
                    .into_iter()
                    .map(|addr| {
                        TcpStream::connect(&addr, &handle)
                        .map(move |stream| (stream, addr))
                    })
                    .collect::<Vec<_>>()
                };
                stream::futures_unordered(connectors)
            };

            let connections = if let Some(listen_socket) = our_info.hole_punch_socket {
                let hole_punch_connections = {
                    nat::tcp_hole_punch(&handle, listen_socket, &their_hole_punch)
                    .map_err(ConnectError::Io)
                }?;
                direct_connections
                .select(hole_punch_connections)
                .into_boxed()
            } else {
                direct_connections
                .into_boxed()
            };

            connections
            .map_err(SingleConnectionError::Io)
            .map(move |(stream, addr)| {
                Socket::wrap_tcp(&handle, stream, addr)
            })
            .and_then(move |socket| {
                socket
                .send((0, HandshakeMessage::Connect(our_connect_request.clone())))
                .map_err(SingleConnectionError::Socket)
            })
            .map(|socket| {
                socket
                .into_future()
                .map_err(|(err, _socket)| SingleConnectionError::Socket(err))
            })
            .buffer_unordered(128)
            .and_then(move |(msg_opt, socket)| {
                match msg_opt {
                    None => Err(SingleConnectionError::ConnectionDropped),
                    Some(HandshakeMessage::Connect(connect_request)) => {
                        let their_uid = validate_connect_request(our_uid, name_hash, connect_request)?;
                        Ok((socket, their_uid))
                    },
                    Some(_msg) => Err(SingleConnectionError::UnexpectedMessage),
                }
            })
        };

        let all_connections = direct_incoming.select(other_connections);

        let chosen_peer = if our_uid > their_uid {
            let handle = handle.clone();
            all_connections
            .first_ok()
            .map_err(ConnectError::AllConnectionsFailed)
            .and_then(move |(socket, their_uid)| {
                socket
                .send((0, HandshakeMessage::ChooseConnection))
                .map_err(ConnectError::ChooseConnection)
                .and_then(move |socket| {
                    peer::from_handshaken_socket(&handle, socket, their_uid, CrustUser::Node)
                    .map_err(ConnectError::Io)
                })
            })
            .into_boxed()
        } else {
            let handle = handle.clone();
            all_connections
            .map(move |(socket, their_uid)| {
                let handle = handle.clone();
                socket
                .into_future()
                .map_err(|(err, _socket)| SingleConnectionError::Socket(err))
                .and_then(move |(msg_opt, socket)| {
                    match msg_opt {
                        None => Err(SingleConnectionError::ConnectionDropped),
                        Some(HandshakeMessage::ChooseConnection) => {
                            peer::from_handshaken_socket(&handle, socket, their_uid, CrustUser::Node)
                            .map_err(SingleConnectionError::Io)
                        },
                        Some(_msg) => Err(SingleConnectionError::UnexpectedMessage),
                    }
                })
            })
            .buffer_unordered(128)
            .first_ok()
            .map_err(ConnectError::AllConnectionsFailed)
            .into_boxed()
        };

        let ret = {
            let timeout = {
                Timeout::new(Duration::from_secs(TIMEOUT_SEC), &handle)
                .map_err(ConnectError::Io)
            }?
            .map_err(ConnectError::Io);

            chosen_peer
            .until(timeout)
            .and_then(|peer_opt| {
                match peer_opt {
                    Some(peer) => Ok(peer),
                    None => Err(ConnectError::TimedOut),
                }
            })
            .into_boxed()
        };

        Ok(ret)
    };

    future::result(try()).flatten().into_boxed()
}

fn validate_connect_request<UID: Uid>(
    expected_uid: UID,
    our_name_hash: NameHash,
    connect_request: ConnectRequest<UID>,
) -> Result<UID, SingleConnectionError> {
    let ConnectRequest { uid: their_uid, name_hash: their_name_hash } = connect_request;
    if their_uid != expected_uid {
        return Err(SingleConnectionError::InvalidUid(format!("{:?}", their_uid)));
    }
    if our_name_hash != their_name_hash {
        return Err(SingleConnectionError::InvalidNameHash(their_name_hash));
    }
    Ok(their_uid)
}

