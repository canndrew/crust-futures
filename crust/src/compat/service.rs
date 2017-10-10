use priv_prelude::*;

use error::CrustError;
use compat::{event_loop, EventLoop, CrustEventSender};

/// This type is a compatibility layer to provide a message-passing API over the underlying
/// futures-based implementation of Service.
pub struct Service<UID: Uid> {
    event_loop: EventLoop<UID>,
}

pub enum ServiceCommand<UID> {
    Zoom(Void, PhantomData<UID>),
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

    /*
    pub fn start_service_discovery(&mut self) {
    }

    pub fn set_service_discovery_listen(&self, listen: bool) {

    }
    */
}

