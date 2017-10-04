use futures::Future;
use until::Until;
use infallible::Infallible;

use BoxFuture;

use void::Void;

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
}

impl<T: Future + Sized> FutureExt for T {}

