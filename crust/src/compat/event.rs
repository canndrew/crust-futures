use uid::Uid;

#[derive(Debug)]
pub enum Event<UID: Uid> {
    _Dummy(UID),
}

