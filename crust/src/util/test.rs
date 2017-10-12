use priv_prelude::*;

use rand::{self, Rng};

pub type UniqueId = [u8; 20];
impl Uid for UniqueId {}

pub fn random_id() -> UniqueId {
    rand::thread_rng().gen()
}

pub fn random_vec(size: usize) -> Vec<u8> {
    let mut ret = Vec::with_capacity(size);
    unsafe {
        ret.set_len(size)
    };
    rand::thread_rng().fill_bytes(&mut ret[..]);
    ret
}

