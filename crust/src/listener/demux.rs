pub struct Demux<UID: Uid> {
    inner: Arc<DemuxInner<UID>>,
}

struct DemuxInner<UID> {
    bootstrap_handler: Mutex<Option<Sender<Peer<UID>>>>,
    connection_handler: Mutex<HashMap<UID, Sender<Peer<UID>>>>,
}

impl Demux<UID: Uid> {
    pub fn new(handle: &Handle, incoming: SocketIncoming<UID>) -> Demux<UID> {
        handle.spawn(incoming.for_each(|socket| {
            handle_incoming(inner, socket)
        }));
        Demux {
            boostrap_handler: Mutex::new(None),
            connection_handle: Mutex::new(HashMap::new()),
        }
    }
}

fn handle_incoming<UID>(inner: DemuxInner<UID>, socket: Socket<UID>) -> BoxFuture<Item=(), Error=Void> {
    socket
    .into_future()
    .and_then(|msg_opt, socket| {
        let msg = match msg_opt {
            Some(msg) => msg,
            None => return future::ok(()).into_boxed(),
        };
        match msg {
            SocketMessage::BootstrapRequest(their_uid, name_hash, ext_reachability) => {
                match unwrap!(inner.bootstrap_handle.lock()).as_ref() {
                    Some(sender) => sender.send(),
                }
            },
            _ => future::ok(()).into_boxed()
        }
    })
    .into_boxed()
}

