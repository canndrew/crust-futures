use futures::sync::mpsc::UnboundedReceiver;
use future_utils::{self, DropNotify};
use service_discovery;
use priv_prelude::*;

pub const SERVICE_DISCOVERY_DEFAULT_PORT: u16 = 5484;

pub struct ServiceDiscovery {
    drop_tx: DropNotify,
}

impl ServiceDiscovery {
    pub fn new(
        handle: &Handle,
        config: ConfigFile,
        current_addrs: HashSet<SocketAddr>,
        addrs_rx: UnboundedReceiver<HashSet<SocketAddr>>,
    ) -> io::Result<ServiceDiscovery> {
        let port = config.read().service_discovery_port.unwrap_or(
            SERVICE_DISCOVERY_DEFAULT_PORT,
        );

        let (drop_tx, drop_rx) = future_utils::drop_notify();
        let mut server = service_discovery::Server::new(handle, port, current_addrs.into_iter().collect::<Vec<_>>())?;

        handle.spawn({
            addrs_rx
            .for_each(move |addrs| {
                server.set_data(addrs.into_iter().collect());
                Ok(())
            })
            .until(drop_rx.infallible())
            .map(|_unit_opt| ())
        });

        Ok(ServiceDiscovery {
            drop_tx,
        })
    }
}

