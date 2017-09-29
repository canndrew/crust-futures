use maidsafe_utilities;

pub use self::event::Event;
pub use self::service::Service;

mod event;
mod service;

/// Used to receive events from a `Service`.
pub type CrustEventSender<UID> = maidsafe_utilities::event_sender::MaidSafeObserver<Event<UID>>;

