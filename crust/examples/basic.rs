#[macro_use]
extern crate unwrap;
extern crate tokio_core;
extern crate futures;
extern crate serde_json;

extern crate crust_futures;

use std::io;
use std::path::PathBuf;

use futures::future::empty;
use tokio_core::reactor::Core;

use crust_futures::{Service, util, ConfigFile, net};

fn main() {
    let mut event_loop = unwrap!(Core::new());
    let service_id = util::random_id();
    println!("Service id: {}", service_id);

    let config = ConfigFile::open_path(PathBuf::from("sample.config"))
        .expect("Failed to read crust config file: sample.config");
    let make_service = Service::with_config(&event_loop.handle(), config, service_id);
    let service = event_loop.run(make_service).expect(
        "Failed to create Service object",
    );

    let listener = event_loop.run(service.start_listener()).expect(
        "Failed to start listening to peers",
    );
    println!("Listening on {}", listener.addr());

    let our_conn_info = event_loop.run(service.prepare_connection_info()).expect(
        "Failed to prepare connection info",
    );
    let pub_conn_info = our_conn_info.pub_connection_info();
    println!(
        "Public connection information:\n{}\n",
        unwrap!(serde_json::to_string(&pub_conn_info))
    );

    println!("Enter remote peer public connection info:");
    let their_info = readln();
    let their_info: net::PubConnectionInfo<util::UniqueId> =
        unwrap!(serde_json::from_str(&their_info));

    let peer = event_loop
        .run(service.connect(our_conn_info, their_info))
        .expect("Failed to connect to given peer");
    println!("Connected to peer: {}", peer.uid());

    // Run event loop forever.
    let res = event_loop.run(empty::<(), ()>());
    unwrap!(res);
}

fn readln() -> String {
    let mut ln = String::new();
    unwrap!(io::stdin().read_line(&mut ln));
    String::from(ln.trim())
}
