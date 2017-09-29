use std::fmt;
use std::hash::Hash;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Trait for specifying a unique identifier for a Crust peer
pub trait Uid
    : 'static
    + Send
    + fmt::Debug
    + Clone
    + Copy
    + Eq
    + PartialEq
    + Ord
    + PartialOrd
    + Hash
    + Serialize
    + DeserializeOwned {
}

