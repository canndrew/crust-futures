use std::{io, mem};
use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use std::marker::PhantomData;
use tokio_core::reactor::Handle;
use tokio_core::net::{UdpSocket, UdpFramed};
use tokio_utils::SerdeUdpCodec;
use futures::stream::StreamFuture;
use futures::sink;
use futures::{Async, Future, Stream, Sink};
use future_utils::{self, DropNotify, FutureExt};
use serde::Serialize;
use serde::de::DeserializeOwned;
use void::{self, Void};

use msg::DiscoveryMsg;

pub struct Server<T>
where
    T: Serialize + DeserializeOwned + Clone + 'static
{
    _ph: PhantomData<T>,
    _drop_notify: DropNotify,
}

struct ServerTask<T>
where
    T: Serialize + DeserializeOwned + Clone + 'static
{
    data: T,
    state: ServerTaskState<T>,
}

enum ServerTaskState<T>
where
    T: Serialize + DeserializeOwned + Clone + 'static
{
    Reading {
        reading: StreamFuture<UdpFramed<SerdeUdpCodec<DiscoveryMsg<T>>>>,
    },
    Writing {
        writing: sink::Send<UdpFramed<SerdeUdpCodec<DiscoveryMsg<T>>>>,
    },
    Invalid,
}

impl<T> Server<T>
where
    T: Serialize + DeserializeOwned + Clone + 'static
{
    pub fn new(
        handle: &Handle,
        port: u16,
        data: T,
    ) -> io::Result<Server<T>> {
        let bind_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port));
        let socket = UdpSocket::bind(&bind_addr, handle)?;
        let framed = socket.framed(SerdeUdpCodec::new());

        let (drop_notify, drop_notice) = future_utils::drop_notify();

        let state = ServerTaskState::Reading {
            reading: framed.into_future(),
        };
        let server_task = ServerTask {
            data: data,
            state: state,
        };
        let server = server_task
            .until(drop_notice)
            .map(|opt| match opt {
                Some(v) => void::unreachable(v),
                None => (),
            })
            .map_err(|v| void::unreachable(v));
        handle.spawn(server);

        let server_ctl = Server {
            _ph: PhantomData,
            _drop_notify: drop_notify,
        };
        Ok(server_ctl)
    }

    /*
    pub fn swap_data(&mut self, data: T) -> T {
    }
    */
}

impl<T> Future for ServerTask<T>
where
    T: Serialize + DeserializeOwned + Clone + 'static
{
    type Item = Void;
    type Error = Void;

    fn poll(&mut self) -> Result<Async<Void>, Void> {
        let mut state = mem::replace(&mut self.state, ServerTaskState::Invalid);
        loop {
            match state {
                ServerTaskState::Reading { mut reading } => {
                    if let Async::Ready((res, framed)) = unwrap!(reading.poll().map_err(|(e, _)| e)) {
                        match res {
                            Some((addr, Ok(DiscoveryMsg::Request))) => {
                                let response = DiscoveryMsg::Response(self.data.clone());
                                let writing = framed.send((addr, response));
                                state = ServerTaskState::Writing { writing };
                                continue;
                            },
                            Some((_, Ok(..))) => (),
                            Some((addr, Err(e))) => {
                                warn!("Error deserialising message from {}: {}", addr, e);
                            },
                            None => unreachable!(),
                        }
                        state = ServerTaskState::Reading { reading: framed.into_future() };
                        continue;
                    } else {
                        state = ServerTaskState::Reading { reading };
                        break;
                    }
                },
                ServerTaskState::Writing { mut writing } => {
                    if let Async::Ready(framed) = unwrap!(writing.poll()) {
                        state = ServerTaskState::Reading { reading: framed.into_future() };
                        continue;
                    } else {
                        state = ServerTaskState::Writing { writing };
                        break;
                    };
                },
                ServerTaskState::Invalid => panic!(),
            }
        }
        self.state = state;
        Ok(Async::NotReady)
    }
}

