use futures::{Future, Stream};
use log::LogLevel;

use until::Until;
use first_ok::FirstOk;
use log_errors::LogErrors;
use BoxStream;

pub trait StreamExt: Stream + Sized {
    fn into_boxed(self) -> BoxStream<Self::Item, Self::Error>
    where
        Self: 'static
    {
        Box::new(self)
    }

    fn until<C>(self, condition: C) -> Until<Self, C>
    where
        C: Future<Item=()>,
        Self::Error: From<C::Error>
    {
        Until::new(self, condition)
    }

    fn first_ok(self) -> FirstOk<Self> {
        FirstOk::new(self)
    }

    fn log_errors(self, level: LogLevel, description: &'static str) -> LogErrors<Self> {
        LogErrors::new(self, level, description)
    }
}

impl<T: Stream + Sized> StreamExt for T {}

