use uid::Uid;
use config::{self, ConfigFile};
use error::CrustError;
use compat::CrustEventSender;

pub struct Service<UID: Uid> {
    event_tx: CrustEventSender<UID>,
}

impl<UID: Uid> Service<UID> {
    /*
    /// Construct a service. `event_tx` is the sending half of the channel which crust will send
    /// notifications on.
    pub fn new(event_tx: CrustEventSender<UID>, our_uid: UID) -> Result<Service<UID>, CrustError> {
        Service::with_config(event_tx, ConfigFile::open_default()?, our_uid)
    }

    /// Constructs a service with the given config. User needs to create an asynchronous channel,
    /// and provide the sender half to this method. Receiver will receive all `Event`s from this
    /// library.
    pub fn with_config(
        event_tx: CrustEventSender<UID>,
        config: ConfigFile,
        our_uid: UID,
    ) -> Result<Service<UID>, CrustError> {
        Ok(Service {
            event_tx: event_tx,
        })
    }

    pub fn start_service_discovery(&mut self) {
    }

    pub fn set_service_discovery_listen(&self, listen: bool) {

    }
    */
}

