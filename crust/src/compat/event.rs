use priv_prelude::*;

#[derive(Debug)]
pub enum Event<UID: Uid> {
    _Dummy(UID),
}

