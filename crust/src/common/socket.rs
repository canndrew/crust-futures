use std::time::Instant;
use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use tokio_core::reactor::Handle;
use tokio_core::net::TcpStream;
use tokio_io::codec::length_delimited::{self, Framed};
use futures::{Async, AsyncSink, Future, Stream, Sink};
use futures::stream::{SplitStream, SplitSink};
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use bytes::BytesMut;
use maidsafe_utilities::serialisation::{serialise_into, deserialise, SerialisationError};

use common::{MAX_PAYLOAD_SIZE, SocketMessage};
use uid::Uid;

pub type Priority = u8;

/// Minimum priority for droppable messages. Messages with lower values will never be dropped.
pub const MSG_DROP_PRIORITY: u8 = 2;
/// Maximum age of a message waiting to be sent. If a message is older, the queue is dropped.
const MAX_MSG_AGE_SECS: u64 = 60;

quick_error! {
    /// Errors that can occur on sockets.
    #[derive(Debug)]
    pub enum SocketError {
        Destroyed {
            description("Socket has been destroyed")
        }
        Io(e: io::Error) {
            description("Io error on socket")
            display("Io error on socket: {}", e)
            cause(e)
            from()
        }
        Deserialisation(e: SerialisationError) {
            description("Error deserialising message from socket")
            display("Error deserialising message from socket")
            cause(e)
            from()
        }
    }
}

pub struct Socket<UID: Uid> {
    inner: Option<Inner<UID>>,
}

pub struct Inner<UID: Uid> {
    stream_rx: SplitStream<Framed<TcpStream>>,
    write_tx: UnboundedSender<(Priority, BytesMut)>,
    peer_addr: SocketAddr,
    _ph: PhantomData<UID>,
}

struct SocketTask<UID: Uid> {
    stream_tx: SplitSink<Framed<TcpStream>>,
    write_queue: BTreeMap<Priority, VecDeque<(Instant, BytesMut)>>,
    write_rx: UnboundedReceiver<(Priority, BytesMut)>,
    _ph: PhantomData<UID>,
}

impl<UID: Uid> Socket<UID> {
    pub fn wrap_tcp(handle: &Handle, stream: TcpStream) -> io::Result<Socket<UID>> {
        let peer_addr = stream.peer_addr()?;
        let framed = length_delimited::Builder::new()
            .max_frame_length(MAX_PAYLOAD_SIZE)
            .new_framed(stream);
        let (stream_tx, stream_rx) = framed.split();
        let (write_tx, write_rx) = mpsc::unbounded();
        let task: SocketTask<UID> = SocketTask {
            stream_tx: stream_tx,
            write_queue: BTreeMap::new(),
            write_rx: write_rx,
            _ph: PhantomData,
        };
        handle.spawn(task.map_err(|e| {
            error!("Socket task failed!: {}", e);
        }));
        let inner = Inner {
            stream_rx: stream_rx,
            write_tx: write_tx,
            peer_addr: peer_addr,
            _ph: PhantomData,
        };
        Ok(Socket {
            inner: Some(inner),
        })
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, SocketError> {
        match self.inner {
            Some(ref inner) => Ok(inner.peer_addr),
            None => Err(SocketError::Destroyed),
        }
    }
}

impl<UID: Uid> Stream for Socket<UID> {
    type Item = SocketMessage<UID>;
    type Error = SocketError;

    fn poll(&mut self) -> Result<Async<Option<SocketMessage<UID>>>, SocketError> {
        let mut inner = match self.inner.take() {
            Some(inner) => inner,
            None => return Err(SocketError::Destroyed),
        };
        let ret = if let Async::Ready(data_opt) = inner.stream_rx.poll()? {
            let data = match data_opt {
                Some(data) => data,
                None => return Err(SocketError::Destroyed),
            };
            let msg = deserialise(&data[..])?;
            Ok(Async::Ready(Some(msg)))
        } else {
            Ok(Async::NotReady)
        };
        self.inner = Some(inner);
        ret
    }
}

impl<UID: Uid> Sink for Socket<UID> {
    type SinkItem = (Priority, SocketMessage<UID>);
    type SinkError = SocketError;

    fn start_send(
        &mut self,
        (priority, msg): (Priority, SocketMessage<UID>),
    ) -> Result<AsyncSink<(Priority, SocketMessage<UID>)>, SocketError> {
        let inner = match self.inner {
            Some(ref mut inner) => inner,
            None => return Err(SocketError::Destroyed),
        };
        let mut data = Vec::with_capacity(1024);
        unwrap!(serialise_into(&msg, &mut data));
        let data = BytesMut::from(data);
        let _ = inner.write_tx.unbounded_send((priority, data));
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Result<Async<()>, SocketError> {
        Ok(Async::Ready(()))
    }
}

impl<UID: Uid> Future for SocketTask<UID> {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> io::Result<Async<()>> {
        let now = Instant::now();
        let socket_dropped = loop {
            match unwrap!(self.write_rx.poll()) {
                Async::Ready(Some((priority, data))) => {
                    let queue = self.write_queue.entry(priority).or_insert_with(|| VecDeque::new());
                    queue.push_back((now, data));
                },
                Async::Ready(None) => {
                    break true;
                },
                Async::NotReady => {
                    break false;
                },
            }
        };

        let expired_keys: Vec<u8> = self.write_queue
            .iter()
            .skip_while(|&(&priority, queue)| {
                priority < MSG_DROP_PRIORITY || // Don't drop high-priority messages.
                queue.front().map_or(true, |&(ref timestamp, _)| {
                    timestamp.elapsed().as_secs() <= MAX_MSG_AGE_SECS
                })
            })
            .map(|(&priority, _)| priority)
            .collect();
        let dropped_msgs: usize = expired_keys
            .iter()
            .filter_map(|priority| self.write_queue.remove(priority))
            .map(|queue| queue.len())
            .sum();
        if dropped_msgs > 0 {
            trace!(
                "Insufficient bandwidth. Dropping {} messages with priority >= {}.",
                dropped_msgs,
                expired_keys[0]
            );
        }

        let mut all_messages_sent = true;
        'outer: for (_, queue) in self.write_queue.iter_mut() {
            while let Some((time, msg)) = queue.pop_front() {
                match self.stream_tx.start_send(msg)? {
                    AsyncSink::Ready => (),
                    AsyncSink::NotReady(msg) => {
                        queue.push_front((time, msg));
                        all_messages_sent = false;
                        break 'outer;
                    },
                }
            }
        }

        if let Async::Ready(()) = self.stream_tx.poll_complete()? {
            if socket_dropped && all_messages_sent {
                return Ok(Async::Ready(()));
            }
        }

        Ok(Async::NotReady)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use tokio_core::reactor::Core;
    use tokio_core::net::{TcpStream, TcpListener};
    use futures::{future, stream, Future, Stream, Sink};
    use void::Void;
    use rand::{self, Rng};
    use env_logger;

    use uid::Uid;
    use common::{Socket, SocketMessage};
    use util;

    #[test]
    fn test_socket() {
        impl Uid for u64 {}

        let _ = env_logger::init();

        let mut core = unwrap!(Core::new());
        let handle = core.handle();
        let res: Result<_, Void> = core.run(future::lazy(move || {
            let listener = unwrap!(TcpListener::bind(&addr!("0.0.0.0:0"), &handle));
            let addr = unwrap!(listener.local_addr());

            let num_msgs = 1000;
            let mut msgs = Vec::with_capacity(num_msgs);
            for _ in 0..num_msgs {
                let size = rand::thread_rng().gen_range(0, 10000);
                let data = util::random_vec(size);
                let msg = SocketMessage::Data(data);
                msgs.push(msg);
            }

            let msgs_send: Vec<_> = msgs.iter().cloned().map(|m| (1, m)).collect();

            let handle0 = handle.clone();
            let f0 = TcpStream::connect(&addr, &handle)
                .map_err(|err| SocketError::from(err))
                .and_then(move |stream| {
                    let socket = Socket::<u64>::wrap_tcp(&handle0, stream);
                    socket.send_all(stream::iter_ok::<_, SocketError>(msgs_send))
                        .map(|(_, _)| ())
                });

            let handle1 = handle.clone();
            let f1 = listener.incoming().into_future()
                .map_err(|(err, _)| SocketError::from(err))
                .and_then(move |(stream_opt, _)| {
                    let (stream, _) = unwrap!(stream_opt);
                    let socket = Socket::<u64>::wrap_tcp(&handle1, stream);
                    socket 
                        .take(num_msgs as u64)
                        .collect()
                        .map(move |msgs_recv| {
                            assert!(msgs_recv == msgs);
                        })
                });

            f0.join(f1)
                .and_then(|((), ())| Ok(()))
                .map_err(|e| panic!(e))
        }));
        unwrap!(res);
    }
}

