use rand::{self, Rng};

#[cfg(test)]
pub fn random_vec(size: usize) -> Vec<u8> {
    let mut ret = Vec::with_capacity(size);
    unsafe {
        ret.set_len(size)
    };
    rand::thread_rng().fill_bytes(&mut ret[..]);
    ret
}

