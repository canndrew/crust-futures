use std::time::Instant;
use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::marker::PhantomData;
use tokio_core::reactor::Handle;
use tokio_core::net::TcpStream;
use tokio_io::codec::length_delimited::{self, Framed};
use futures::{Async, AsyncSink, Future, Stream, Sink};
use futures::stream::{SplitStream, SplitSink};
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};
use bytes::BytesMut;
use maidsafe_utilities::serialisation::{serialise_into, deserialise, SerialisationError};

use common::{MAX_PAYLOAD_SIZE, Message};
use uid::Uid;

pub type Priority = u8;

/// Minimum priority for droppable messages. Messages with lower values will never be dropped.
pub const MSG_DROP_PRIORITY: u8 = 2;
/// Maximum age of a message waiting to be sent. If a message is older, the queue is dropped.
const MAX_MSG_AGE_SECS: u64 = 60;

quick_error! {
    /// Errors raised by sockets
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
            description("Error deserialising message from peer")
            display("Error deserialising message from peer")
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
    _ph: PhantomData<UID>,
}

struct SocketTask<UID: Uid> {
    stream_tx: SplitSink<Framed<TcpStream>>,
    write_queue: BTreeMap<Priority, VecDeque<(Instant, BytesMut)>>,
    write_rx: UnboundedReceiver<(Priority, BytesMut)>,
    _ph: PhantomData<UID>,
}

impl<UID: Uid> Socket<UID> {
    pub fn wrap_tcp(handle: &Handle, stream: TcpStream) -> Socket<UID> {
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
            _ph: PhantomData,
        };
        Socket {
            inner: Some(inner),
        }
    }
}

impl<UID: Uid> Stream for Socket<UID> {
    type Item = Message<UID>;
    type Error = SocketError;

    fn poll(&mut self) -> Result<Async<Option<Message<UID>>>, SocketError> {
        let inner = match self.inner {
            Some(ref mut inner) => inner,
            None => return Err(SocketError::Destroyed),
        };
        if let Async::Ready(data_opt) = inner.stream_rx.poll()? {
            let data = unwrap!(data_opt);
            let msg = deserialise(&data[..])?;
            Ok(Async::Ready(Some(msg)))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl<UID: Uid> Sink for Socket<UID> {
    type SinkItem = (Priority, Message<UID>);
    type SinkError = SocketError;

    fn start_send(
        &mut self,
        (priority, msg): (Priority, Message<UID>),
    ) -> Result<AsyncSink<(Priority, Message<UID>)>, SocketError> {
        let inner = match self.inner {
            Some(ref mut inner) => inner,
            None => return Err(SocketError::Destroyed),
        };
        let mut data = BytesMut::with_capacity(1024);
        unwrap!(serialise_into(&msg, &mut data));
        let _ = inner.write_tx.send((priority, data));
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
        for (priority, queue) in self.write_queue.iter_mut() {
            let mut put_back = None;
            for (time, msg) in queue.drain(..) {
                match self.stream_tx.start_send(msg)? {
                    AsyncSink::Ready => (),
                    AsyncSink::NotReady(msg) => {
                        put_back = Some((time, msg));
                        break;
                    },
                }
            }
            if let Some((time, msg)) = put_back {
                queue.push_front((time, msg));
                all_messages_sent = false;
                break;
            }
        }

        if Async::Ready(()) = self.stream_tx.poll_complete()? {
            if socket_dropped && all_messages_sent {
                return Ok(Async::Ready(()));
            }
        }

        Ok(Async::NotReady)
    }
}

