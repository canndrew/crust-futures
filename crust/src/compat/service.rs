use std;
use std::sync::{Arc, Mutex};
use future_utils::{self, DropNotify};
use futures::sync::mpsc::{self, UnboundedSender};
use futures::stream::{SplitSink};
use log::LogLevel;
use void;
use serde;

use priv_prelude::*;

use net::BootstrapAcceptor;
use error::CrustError;
use compat::{event_loop, EventLoop, CrustEventSender};
use compat::{Event, ConnectionInfoResult};
use service;
use service_discovery;

pub trait FnBox<UID: Uid> {
    fn call_box(self: Box<Self>, handle: &Handle, state: &mut ServiceState<UID>);
}

impl<UID: Uid, F: FnOnce(&Handle, &mut ServiceState<UID>)> FnBox<UID> for F {
    fn call_box(self: Box<Self>, handle: &Handle, state: &mut ServiceState<UID>) {
        (*self)(handle, state)
    }
}

pub type ServiceCommand<UID> = Box<FnBox<UID> + Send>;

/// This type is a compatibility layer to provide a message-passing API over the underlying
/// futures-based implementation of Service.
pub struct Service<UID: Uid> {
    event_loop: EventLoop<UID>,
}

impl<UID: Uid> Service<UID> {
    /// Construct a service. `event_tx` is the sending half of the channel which crust will send
    /// notifications on.
    pub fn new(event_tx: CrustEventSender<UID>, our_uid: UID) -> Result<Service<UID>, CrustError> {
        let config = ConfigFile::open_default()?;
        Service::with_config(event_tx, config, our_uid)
    }

    /// Constructs a service with the given config. User needs to create an asynchronous channel,
    /// and provide the sender half to this method. Receiver will receive all `Event`s from this
    /// library.
    pub fn with_config(
        event_tx: CrustEventSender<UID>,
        config: ConfigFile,
        our_uid: UID,
    ) -> Result<Service<UID>, CrustError> {
        let event_loop_id = Some(format!("{:?}", our_uid));
        let event_loop = event_loop::spawn_event_loop(event_loop_id.as_ref().map(|s| s.as_ref()), event_tx, our_uid, config)?;
        Ok(Service {
            event_loop,
        })
    }

    pub fn start_service_discovery(&mut self) {
    }

    /*
    pub fn set_service_discovery_listen(&self, listen: bool) {
        self.event_loop.send(Box::new(move |handle: &Handle, state: &mut ServiceState<UID>| {
            let port = state.service.config().read().service_discovery_port.unwrap_or(
                service::SERVICE_DISCOVERY_DEFAULT_PORT,
            );

            let listen_addrs = state.service.addresses();
            let sd = service_discovery::Server::new(handle, port, listen_addrs);

            let (tx, rx) = mpsc::unbounded();
            handle.spawn(rx.for_each(move |listen_addrs| {
                sd.set_data(listen_addrs);
            }));

            state.service_discovery = Some(tx);
        }));
    }
    */

    pub fn set_accept_bootstrap(&self, accept: bool) -> Result<(), CrustError> {
        self.event_loop.send(Box::new(move |handle: &Handle, state: &mut ServiceState<UID>| {
            if accept {
                if state.bootstrap_acceptor.is_none() {
                    let handle0 = handle.clone();
                    let handle1 = handle.clone();
                    let (drop_tx, drop_rx) = future_utils::drop_notify();
                    state.bootstrap_acceptor = Some(drop_tx);
                    let acceptor = state.service.bootstrap_acceptor();
                    let cm = state.cm.clone();
                    let event_tx = state.event_tx.clone();
                    handle0.spawn({
                        acceptor
                        .log_errors(LogLevel::Info, "accepting bootstrap connection")
                        .until(drop_rx)
                        .for_each(move |peer| {
                            let their_uid = peer.uid();
                            let their_kind = peer.kind();
                            let their_addr = match peer.addr() {
                                Ok(addr) => addr,
                                Err(e) => {
                                    error!("error getting address of bootstrapping peer: {}", e);
                                    return Ok(());
                                },
                            };
                            let mut cm = unwrap!(cm.lock());
                            cm.insert_peer(&handle1, peer, their_addr);
                            let _ = event_tx.send(Event::BootstrapAccept(their_uid, their_kind));
                            Ok(())
                        })
                        .infallible()
                    });
                }
            } else {
                let _ = state.bootstrap_acceptor.take();
            }
        }));
        Ok(())
    }

    pub fn get_peer_socket_addr(&self, peer_uid: UID) -> Result<SocketAddr, CrustError> {
        let (tx, rx) = std::sync::mpsc::channel::<Result<SocketAddr, CrustError>>();
        self.event_loop.send(Box::new(move |_handle: &Handle, state: &mut ServiceState<UID>| {
            let cm = unwrap!(state.cm.lock());
            unwrap!(tx.send(cm.peer_addr(&peer_uid)));
        }));
        unwrap!(rx.recv())
    }

    pub fn get_peer_ip_addr(&self, peer_uid: UID) -> Result<IpAddr, CrustError> {
        self.get_peer_socket_addr(peer_uid).map(|a| a.ip())
    }

    pub fn is_peer_hard_coded(&self, peer_uid: UID) -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        self.event_loop.send(Box::new(move |_handle: &Handle, state: &mut ServiceState<UID>| {
            let config = state.service.config();
            let cm = unwrap!(state.cm.lock());
            let res = {
                cm
                .peer_addr(&peer_uid)
                .map(|peer_addr| {
                    config.read().hard_coded_contacts.iter().any(
                        |addr| addr.ip() == peer_addr.ip()
                    )
                })
                .unwrap_or(false)
            };
            unwrap!(tx.send(res));
        }));
        unwrap!(rx.recv())
    }

    pub fn start_bootstrap(
        &self,
        blacklist: HashSet<SocketAddr>,
        crust_user: CrustUser,
    ) -> Result<(), CrustError> {
        self.event_loop.send(Box::new(move |handle: &Handle, state: &mut ServiceState<UID>| {
            let (drop_tx, drop_rx) = future_utils::drop_notify();
            state.bootstrap_connect = Some(drop_tx);
            let cm = state.cm.clone();
            let event_tx = state.event_tx.clone();
            let f = {
                let handle = handle.clone();
                state.service
                .bootstrap(blacklist, crust_user)
                .map_err(|e| error!("bootstrap failed: {}", e))
                .and_then(move |peer| {
                    let addr = {
                        peer
                        .addr()
                        .map_err(|e| error!("failed to get address of peer we bootstrapped to: {}", e))
                    }?;
                    let uid = peer.uid();
                    let mut cm = unwrap!(cm.lock());
                    cm.insert_peer(&handle, peer, addr);
                    Ok((addr, uid))
                })
                .then(move |res| {
                    let event = match res {
                        Ok((addr, uid)) => Event::BootstrapConnect(uid, addr),
                        Err(()) => Event::BootstrapFailed,
                    };
                    let _ = event_tx.send(event);
                    Ok(())
                })
                .until(drop_rx.infallible())
                .map(|_| ())
            };
            handle.spawn(f);
        }));
        Ok(())
    }

    pub fn stop_bootstrap(&self) -> Result<(), CrustError> {
        self.event_loop.send(Box::new(move |_handle: &Handle, state: &mut ServiceState<UID>| {
            let _ = state.bootstrap_connect.take();
        }));
        Ok(())
    }

    pub fn start_listening_tcp(&self) -> Result<(), CrustError> {
        self.event_loop.send(Box::new(move |handle: &Handle, state: &mut ServiceState<UID>| {
            if state.tcp_listener.is_some() {
                return;
            }

            let mut listen_addrs = state.service.addresses();
            let listen_addrs_tx_opt = state.service_discovery.clone();
            let (drop_tx, drop_rx) = future_utils::drop_notify();
            state.tcp_listener = Some(drop_tx);
            let f = {
                state.service
                .start_listener()
                .map_err(|e| error!("failed to start listener: {}", e))
                .and_then(move |listener| {
                    let listener_addr = listener.addr();
                    listen_addrs.retain(|addr| *addr != listener_addr);
                    listen_addrs.push(listener_addr);
                    if let Some(listen_addrs_tx) = listen_addrs_tx_opt {
                        let _ = listen_addrs_tx.unbounded_send(listen_addrs);
                    }
                    future::empty::<Void, ()>()
                })
                .until(drop_rx.infallible())
                .map(|void_opt| {
                    match void_opt {
                        Some(v) => void::unreachable(v),
                        None => (),
                    }
                })
            };
            handle.spawn(f)
        }));
        Ok(())
    }

    pub fn stop_tcp_listener(&self) -> Result<(), CrustError> {
        self.event_loop.send(Box::new(move |_handle: &Handle, state: &mut ServiceState<UID>| {
            let _ = state.tcp_listener.take();
        }));
        Ok(())
    }

    pub fn prepare_connection_info(&self, result_token: u32) {
        self.event_loop.send(Box::new(move |handle: &Handle, state: &mut ServiceState<UID>| {
            let event_tx = state.event_tx.clone();
            let f = {
                state.service
                .prepare_connection_info()
                .then(move |result| {
                    let _ = event_tx.send(Event::ConnectionInfoPrepared(ConnectionInfoResult {
                        result_token,
                        result,
                    }));
                    Ok(())
                })
            };
            handle.spawn(f);
        }));
    }

    /// Connect to a peer. To call this method you must follow these steps:
    ///  * Generate a `PrivConnectionInfo` via `Service::prepare_connection_info`.
    ///  * Create a `PubConnectionInfo` via `PrivConnectionInfo::to_pub_connection_info`.
    ///  * Swap `PubConnectionInfo`s out-of-band with the peer you are connecting to.
    ///  * Call `Service::connect` using your `PrivConnectionInfo` and the `PubConnectionInfo`
    ///    obtained from the peer
    pub fn connect(
        &self,
        our_ci: PrivConnectionInfo<UID>,
        their_ci: PubConnectionInfo<UID>,
    ) -> Result<(), CrustError> {
        self.event_loop.send(Box::new(move |handle: &Handle, state: &mut ServiceState<UID>| {
            let uid = their_ci.id;
            let cm = state.cm.clone();
            let event_tx = state.event_tx.clone();
            let f = {
                let handle = handle.clone();
                state.service
                .connect(our_ci, their_ci)
                .map_err(move |e| {
                    error!("connection to {:?} failed: {}", uid, e);
                })
                .and_then(move |peer| {
                    let addr = {
                        peer
                        .addr()
                        .map_err(|e| error!("failed to get address of peer we connected to: {}", e))
                    }?;
                    let mut cm = unwrap!(cm.lock());
                    cm.insert_peer(&handle, peer, addr);
                    Ok(())
                })
                .then(move |res| {
                    let event = match res {
                        Ok(()) => Event::ConnectSuccess(uid),
                        Err(()) => Event::ConnectFailure(uid),
                    };
                    let _ = event_tx.send(event);
                    Ok(())
                })
            };
            handle.spawn(f);
        }));
        Ok(())
    }

    /// Disconnect from the given peer and returns whether there was a connection at all.
    pub fn disconnect(&self, peer_uid: &UID) -> bool {
        let peer_uid = *peer_uid;
        let (tx, rx) = std::sync::mpsc::channel();
        self.event_loop.send(Box::new(move |_handle: &Handle, state: &mut ServiceState<UID>| {
            let mut cm = unwrap!(state.cm.lock());
            unwrap!(tx.send(cm.remove(&peer_uid)));
        }));
        unwrap!(rx.recv())
    }

    /// Send data to a peer.
    pub fn send(&self, peer_uid: &UID, msg: Vec<u8>, priority: Priority) -> Result<(), CrustError> {
        let peer_uid = *peer_uid;
        let (tx, rx) = std::sync::mpsc::channel();
        self.event_loop.send(Box::new(move |_handle: &Handle, state: &mut ServiceState<UID>| {
            let try = move || {
                let mut cm = unwrap!(state.cm.lock());
                let mut peer = cm.get_mut(&peer_uid)?;
                match peer.start_send((priority, msg))? {
                    AsyncSink::NotReady(..) => unreachable!(),
                    AsyncSink::Ready => (),
                };
                Ok(())
            };
            unwrap!(tx.send(try()));
        }));
        unwrap!(rx.recv())
    }

    /// Check if we are connected to the given peer
    pub fn is_connected(&self, peer_uid: &UID) -> bool {
        let peer_uid = *peer_uid;
        let (tx, rx) = std::sync::mpsc::channel();
        self.event_loop.send(Box::new(move |_handle: &Handle, state: &mut ServiceState<UID>| {
            let mut cm = unwrap!(state.cm.lock());
            unwrap!(tx.send(cm.contains_key(&peer_uid)));
        }));
        unwrap!(rx.recv())
    }

    /// Returns our ID.
    pub fn id(&self) -> UID {
        let (tx, rx) = std::sync::mpsc::channel();
        self.event_loop.send(Box::new(move |_handle: &Handle, state: &mut ServiceState<UID>| {
            unwrap!(tx.send(state.service.id()));
        }));
        unwrap!(rx.recv())
    }
}

pub struct ServiceState<UID: Uid> {
    service: ::Service<UID>,
    event_tx: CrustEventSender<UID>,
    cm: Arc<Mutex<ConnectionMap<UID>>>,

    bootstrap_acceptor: Option<DropNotify>,
    bootstrap_connect: Option<DropNotify>,
    tcp_listener: Option<DropNotify>,

    // TODO: it would be better if the acceptor raised a stream of events indicating what
    // addresses its listening on.
    service_discovery: Option<UnboundedSender<Vec<SocketAddr>>>,
}

impl<UID: Uid> ServiceState<UID> {
    pub fn new(service: ::Service<UID>, event_tx: CrustEventSender<UID>) -> ServiceState<UID> {
        let cm = Arc::new(Mutex::new(ConnectionMap {
            map: HashMap::new(),
            event_tx: event_tx.clone(),
        }));
        ServiceState {
            service: service,
            event_tx: event_tx,
            cm: cm,
            bootstrap_acceptor: None,
            bootstrap_connect: None,
            tcp_listener: None,
            service_discovery: None,
        }
    }
}

struct ConnectionMap<UID: Uid> {
    map: HashMap<UID, (SocketAddr, DropNotify, SplitSink<Peer<UID>>)>,
    event_tx: CrustEventSender<UID>,
}

impl<UID: Uid> ConnectionMap<UID> {
    pub fn insert_peer(&mut self, handle: &Handle, peer: Peer<UID>, addr: SocketAddr) {
        let (drop_tx, drop_rx) = future_utils::drop_notify();
        let uid = peer.uid();
        let kind = peer.kind();
        let (peer_sink, peer_stream) = peer.split();

        let event_tx0 = self.event_tx.clone();
        let event_tx1 = self.event_tx.clone();
        handle.spawn({
            peer_stream
            .log_errors(LogLevel::Info, "receiving data from peer")
            .until(drop_rx)
            .for_each(move |msg| {
                let _ = event_tx0.send(Event::NewMessage(uid, kind, msg));
                Ok(())
            })
            .map(move |()| {
                let _ = event_tx1.send(Event::LostPeer(uid));
            })
            .infallible()
        });

        let _ = self.map.insert(uid, (addr, drop_tx, peer_sink));
    }
    
    pub fn get_mut(&mut self, uid: &UID) -> Result<&mut SplitSink<Peer<UID>>, CrustError> {
        self.map
        .get_mut(uid)
        .map(|&mut (_, _, ref mut peer_sink)| Ok(peer_sink))
        .unwrap_or(Err(CrustError::PeerNotFound))
    }
    
    pub fn get(&self, uid: &UID) -> Result<&SplitSink<Peer<UID>>, CrustError> {
        self.map
        .get(uid)
        .map(|&(_, _, ref peer_sink)| Ok(peer_sink))
        .unwrap_or(Err(CrustError::PeerNotFound))
    }

    pub fn peer_addr(&self, uid: &UID) -> Result<SocketAddr, CrustError> {
        self.map
        .get(uid)
        .map(|&(addr, _, _)| Ok(addr))
        .unwrap_or(Err(CrustError::PeerNotFound))
    }

    pub fn remove(&mut self, uid: &UID) -> bool {
        self.map
        .remove(uid)
        .is_some()
    }

    pub fn contains_key(&mut self, uid: &UID) -> bool {
        self.map
        .contains_key(uid)
    }
}

