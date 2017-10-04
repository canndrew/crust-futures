use std::mem;
use futures::{Async, Future, Stream};

pub struct FirstOk<S>
where
    S: Stream,
{
    stream: S,
    errors: Vec<S::Error>,
}

impl<S> FirstOk<S>
where
    S: Stream,
{
    pub fn new(stream: S) -> FirstOk<S> {
        FirstOk {
            stream: stream,
            errors: Vec::new(),
        }
    }
}

impl<S> Future for FirstOk<S>
where
    S: Stream
{
    type Item = S::Item;
    type Error = Vec<S::Error>;

    fn poll(&mut self) -> Result<Async<S::Item>, Vec<S::Error>> {
        loop {
            match self.stream.poll() {
                Ok(Async::Ready(Some(val))) => {
                    self.errors.clear();
                    return Ok(Async::Ready(val));
                },
                Ok(Async::Ready(None)) => {
                    let errors = mem::replace(&mut self.errors, Vec::new());
                    return Err(errors);
                },
                Ok(Async::NotReady) => {
                    return Ok(Async::NotReady);
                },
                Err(e) => {
                    self.errors.push(e);
                },
            }
        }
    }
}

