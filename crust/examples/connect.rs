//! This example demonstrates how to make a P2P connection using `crust`.
//! We are using `crust::Service` to listen for incoming connections
//! and to establish connection to remote peer.
//!
//! In a nutshell connetion looks like this:
//!
//! 1. start listening for incoming connections
//! 2. prepare connection information: public and private
//! 3. exchange public information
//! 4. connect
//!
//! Run two instances of this sample: preferably on separate computers but
//! localhost is fine too.
//! When the sample starts it prints generated public information which
//! is represented as JSON object.
//! Copy this object from first to second peer and hit ENTER.
//! Do the same with the second peer: copy it's public information JSON
//! to first peer and hit ENTER.
//! On both peers you should see something like:
//! ```
//! Connected to peer: 4a755684f72fe63fba86725b80d42d69ed649392
//! ```
//! That's it, it means we successfully did a peer-to-peer connection.

#[macro_use]
extern crate unwrap;
extern crate tokio_core;
extern crate futures;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate rand_derive;
extern crate rand;

extern crate crust_futures;

use std::{io, fmt};
use std::path::PathBuf;

use futures::future::empty;
use tokio_core::reactor::Core;
use rand::Rng;

use crust_futures::{Service, ConfigFile, PubConnectionInfo, Uid};

// Some peer ID boilerplate.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Rand)]
struct PeerId(u64);

impl Uid for PeerId {}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let PeerId(ref id) = *self;
        write!(f, "{:x}", id)
    }
}

fn main() {
    let mut event_loop = unwrap!(Core::new());
    let service_id = rand::thread_rng().gen::<PeerId>();
    println!("Service id: {}", service_id);

    let config = unwrap!(
        ConfigFile::open_path(PathBuf::from("sample.config")),
        "Failed to read crust config file: sample.config",
    );
    let make_service = Service::with_config(&event_loop.handle(), config, service_id);
    let service = unwrap!(
        event_loop.run(make_service),
        "Failed to create Service object",
    );

    let listener = unwrap!(
        event_loop.run(service.start_listener()),
        "Failed to start listening to peers",
    );
    println!("Listening on {}", listener.addr());

    let our_conn_info = unwrap!(
        event_loop.run(service.prepare_connection_info()),
        "Failed to prepare connection info",
    );
    let pub_conn_info = our_conn_info.pub_connection_info();
    println!(
        "Public connection information:\n{}\n",
        unwrap!(serde_json::to_string(&pub_conn_info))
    );

    println!("Enter remote peer public connection info:");
    let their_info = readln();
    let their_info: PubConnectionInfo<PeerId> = unwrap!(serde_json::from_str(&their_info));

    let peer = unwrap!(
        event_loop.run(service.connect(our_conn_info, their_info)),
        "Failed to connect to given peer",
    );
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
