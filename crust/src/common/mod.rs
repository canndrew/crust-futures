use std::net::SocketAddr;

mod error;
mod socket;
mod message;

pub use self::error::CommonError;
pub use self::message::{SocketMessage, PeerMessage, BootstrapDenyReason};
pub use self::socket::{Socket, SocketError};

/// Specify crust user. Behaviour (for example in bootstrap phase) will be different for different
/// variants. Node will request the Bootstrapee to connect back to this crust failing which it
/// would mean it's not reachable from outside and hence should be rejected bootstrap attempts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CrustUser {
    /// Crust user is a Node and should not be allowed to bootstrap if it's not reachable from
    /// outside.
    Node,
    /// Crust user is a Client and should be allowed to bootstrap even if it's not reachable from
    /// outside.
    Client,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExternalReachability {
    NotRequired,
    Required { direct_listeners: Vec<SocketAddr> },
}

pub const HASH_SIZE: usize = 32;
pub type NameHash = [u8; HASH_SIZE];

pub const MAX_PAYLOAD_SIZE: usize = 2 * 1024 * 1024;

