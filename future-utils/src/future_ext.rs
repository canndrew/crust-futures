use std::fmt::Display;
use futures::Future;
use log::LogLevel;
use void::Void;

use log_error::LogError;
use until::Until;
use infallible::Infallible;
use BoxFuture;

pub trait FutureExt: Future + Sized {
    fn into_boxed(self) -> BoxFuture<Self::Item, Self::Error>
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

    fn infallible<E>(self) -> Infallible<Self, E>
    where
        Self: Future<Error=Void>
    {
        Infallible::new(self)
    }

    fn log_error(self, level: LogLevel, description: &'static str) -> LogError<Self>
    where
        Self: Future<Item=()>,
        Self::Error: Display,
    {
        LogError::new(self, level, description)
    }
}

impl<T: Future + Sized> FutureExt for T {}

