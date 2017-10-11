use maidsafe_utilities;

pub use self::event::{Event, ConnectionInfoResult};
pub use self::service::Service;
pub use self::event_loop::EventLoop;

mod event;
mod service;
mod event_loop;

/// Used to receive events from a `Service`.
pub type CrustEventSender<UID> = maidsafe_utilities::event_sender::MaidSafeObserver<Event<UID>>;

