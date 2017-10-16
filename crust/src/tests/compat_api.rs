use std::sync::mpsc::{self, Receiver};
use std::thread;
use maidsafe_utilities::event_sender::{MaidSafeEventCategory, MaidSafeObserver};
use rand;
use env_logger;
use compat::{self, CrustEventSender, Event};
use priv_prelude::*;
use util::{self, UniqueId};
use ::MAX_PAYLOAD_SIZE;

fn event_sender() -> (CrustEventSender<UniqueId>, Receiver<Event<UniqueId>>) {
    let (category_tx, _) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    (
        MaidSafeObserver::new(event_tx, MaidSafeEventCategory::Crust, category_tx),
        event_rx,
    )
}

fn service() -> (compat::Service<UniqueId>, Receiver<Event<UniqueId>>) {
    let (event_tx, event_rx) = event_sender();
    let config = unwrap!(ConfigFile::new_temporary());
    let uid: UniqueId = rand::random();
    let service = unwrap!(compat::Service::with_config(event_tx, config, uid));
    (service, event_rx)
}

#[test]
fn start_two_services_exchange_data() {
    let _ = env_logger::init();

    let (service0, event_rx0) = service();
    let (service1, event_rx1) = service();

    unwrap!(service0.start_listening_tcp());
    let port0 = match unwrap!(event_rx0.recv()) {
        Event::ListenerStarted(port0) => port0,
        m => panic!("Unexpected message: {:?}", m),
    };

    unwrap!(service1.start_listening_tcp());
    let port1 = match unwrap!(event_rx1.recv()) {
        Event::ListenerStarted(port1) => port1,
        m => panic!("Unexpected message: {:?}", m),
    };

    assert!(port0 != port1);

    const NUM_MESSAGES: usize = 100;
    const MAX_DATA_SIZE: usize = MAX_PAYLOAD_SIZE - 8;

    let uid0 = service0.id();
    let uid1 = service1.id();

    let data0 = (0..NUM_MESSAGES).map(|_| util::random_vec(MAX_DATA_SIZE)).collect::<Vec<_>>();
    let data0_compare = data0.clone();

    let data1 = (0..NUM_MESSAGES).map(|_| util::random_vec(MAX_DATA_SIZE)).collect::<Vec<_>>();
    let data1_compare = data1.clone();

    let (ci_tx0, ci_rx0) = mpsc::channel();
    let (ci_tx1, ci_rx1) = mpsc::channel();

    let j0 = thread::spawn(move || {
        let token = rand::random();
        service0.prepare_connection_info(token);
        let ci0 = match unwrap!(event_rx0.recv()) {
            Event::ConnectionInfoPrepared(res) => {
                assert_eq!(res.result_token, token);
                unwrap!(res.result)
            },
            m => panic!("Unexpected message: {:?}", m),
        };
        unwrap!(ci_tx0.send(ci0.pub_connection_info()));
        let pub_ci1 = unwrap!(ci_rx1.recv());

        println!("ci0 == {:?}", ci0);
        println!("pub_ci1 == {:?}", pub_ci1);

        unwrap!(service0.connect(ci0, pub_ci1));
        match unwrap!(event_rx0.recv()) {
            Event::ConnectSuccess(id) => {
                assert_eq!(id, uid1);
            },
            m => panic!("Unexpected message: {:?}", m),
        }

        for payload in data0 {
            unwrap!(service0.send(&uid1, payload, 0));
        }

        let mut i = 0;
        let data1_recv = {
            event_rx0
            .into_iter()
            .take(NUM_MESSAGES)
            .map(|msg| {
                i += 1;
                println!("collected {}/{} msgs", i, NUM_MESSAGES);
                match msg {
                    Event::NewMessage(uid, _user, data) => {
                        assert_eq!(uid, uid1);
                        data
                    },
                    e => panic!("unexpected event: {:?}", e),
                }
            })
            .collect::<Vec<_>>()
        };

        assert!(data1_recv == data1_compare);
        service0
    });
    let j1 = thread::spawn(move || {
        let token = rand::random();
        service1.prepare_connection_info(token);
        let ci1 = match unwrap!(event_rx1.recv()) {
            Event::ConnectionInfoPrepared(res) => {
                assert_eq!(res.result_token, token);
                unwrap!(res.result)
            },
            m => panic!("Unexpected message: {:?}", m),
        };
        unwrap!(ci_tx1.send(ci1.pub_connection_info()));
        let pub_ci0 = unwrap!(ci_rx0.recv());

        unwrap!(service1.connect(ci1, pub_ci0));
        match unwrap!(event_rx1.recv()) {
            Event::ConnectSuccess(id) => {
                assert_eq!(id, uid0);
            },
            m => panic!("Unexpected message: {:?}", m),
        }

        for payload in data1 {
            unwrap!(service1.send(&uid0, payload, 0));
        }

        let mut i = 0;
        let data0_recv = {
            event_rx1
            .into_iter()
            .take(NUM_MESSAGES)
            .map(|msg| {
                i += 1;
                println!("collected {}/{} msgs", i, NUM_MESSAGES);
                match msg {
                    Event::NewMessage(uid, _user, data) => {
                        assert_eq!(uid, uid0);
                        data
                    },
                    e => panic!("unexpected event: {:?}", e),
                }
            })
            .collect::<Vec<_>>()
        };

        assert!(data0_recv == data0_compare);
    });

    let service0 = j0.join();
    let service1 = j1.join();
    drop(service0);
    drop(service1);
}

