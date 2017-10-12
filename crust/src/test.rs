use tokio_core::reactor::Core;
use priv_prelude::*;
use service::Service;
use util;

#[test]
fn test() {
    let mut core = unwrap!(Core::new());
    let handle = core.handle();

    let res = core.run({
        Service::new(&handle, util::random_id())
        .and_then(|_service| Ok(()))
    });
    unwrap!(res);
}

