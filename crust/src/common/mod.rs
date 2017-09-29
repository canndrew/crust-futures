use std::net::SocketAddr;

mod error;
//mod socket;
mod message;

pub use self::error::CommonError;
pub use self::message::Message;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExternalReachability {
    NotRequired,
    Required { direct_listeners: Vec<SocketAddr> },
}

pub const HASH_SIZE: usize = 32;
pub type NameHash = [u8; HASH_SIZE];

pub const MAX_PAYLOAD_SIZE: usize = 2 * 1024 * 1024;

