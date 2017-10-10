use std;
use tokio_core::reactor::Core;
use maidsafe_utilities::thread::{self, Joiner};
use futures::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};

use priv_prelude::*;

use error::CrustError;
use compat::CrustEventSender;
use compat::service::ServiceCommand;

pub struct EventLoop<UID: Uid> {
    tx: UnboundedSender<ServiceCommand<UID>>,
    _joiner: Joiner,
}

impl<UID: Uid> EventLoop<UID> {
    pub fn send(&self, msg: ServiceCommand<UID>) {
        unwrap!(self.tx.unbounded_send(msg));
    }
}

pub fn spawn_event_loop<UID: Uid>(
    event_loop_id: Option<&str>,
    event_tx: CrustEventSender<UID>,
    our_uid: UID,
    config: ConfigFile,
) -> Result<EventLoop<UID>, CrustError> {
    let mut name = "CRUST-Event-Loop".to_string();
    if let Some(id) = event_loop_id {
        name.push_str(": ");
        name.push_str(id);
    }

    let (result_tx, result_rx) = std::sync::mpsc::channel::<Result<_, CrustError>>();

    let joiner = thread::named(name, move || {
        let try = move || {
            let mut core = Core::new()?;
            let handle = core.handle();

            let service = core.run(::Service::with_config(&handle, config, our_uid))?;

            let (tx, rx) = mpsc::unbounded();
            let event_loop_impl = EventLoopImpl {
                service,
                rx,
                event_tx,
            };
            Ok((core, event_loop_impl, tx))
        };

        match try() {
            Ok((mut core, event_loop_impl, tx)) => {
                unwrap!(result_tx.send(Ok(tx)));
                unwrap!(core.run(event_loop_impl));
            },
            Err(e) => {
                unwrap!(result_tx.send(Err(e)));
            },
        }
    });

    let tx = unwrap!(result_rx.recv())?;

    Ok(EventLoop {
        tx: tx,
        _joiner: joiner,
    })
}

struct EventLoopImpl<UID: Uid> {
    service: ::Service<UID>,
    rx: UnboundedReceiver<ServiceCommand<UID>>,
    event_tx: CrustEventSender<UID>,
}

impl<UID: Uid> Future for EventLoopImpl<UID> {
    type Item = ();
    type Error = Void;

    fn poll(&mut self) -> Result<Async<()>, Void> {
        loop {
            match self.rx.poll() {
                Ok(Async::Ready(Some(command))) => self.run(command),
                Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
                Ok(Async::NotReady) => break,
                Err(()) => unreachable!(),
            }
        }
        Ok(Async::NotReady)
    }
}

impl<UID: Uid> EventLoopImpl<UID> {
    fn run(&mut self, command: ServiceCommand<UID>) {
        match command {
            ServiceCommand::Zoom(v, _) => match v {},
        }
    }
}

