use std::thread;
use futures::task::{self, Task};
use futures::sync::BiLock;
use futures::{Async, Future};
use void::Void;

struct Inner {
    dropped: bool,
    task_rx: Option<Task>,
}

pub struct DropNotify {
    inner: BiLock<Inner>,
}

pub struct DropNotice {
    inner: BiLock<Inner>,
}

pub fn drop_notify() -> (DropNotify, DropNotice) {
    let inner = Inner {
        dropped: false,
        task_rx: None,
    };
    let (lock_notify, lock_notice) = BiLock::new(inner);
    let drop_notify = DropNotify {
        inner: lock_notify,
    };
    let drop_notice = DropNotice {
        inner: lock_notice,
    };
    (drop_notify, drop_notice)
}

impl Future for DropNotice {
    type Item = ();
    type Error = Void;

    fn poll(&mut self) -> Result<Async<()>, Void> {
        if let Async::Ready(ref mut inner) = self.inner.poll_lock() {
            if inner.dropped {
                return Ok(Async::Ready(()));
            }
            inner.task_rx = Some(task::current());
            Ok(Async::NotReady)
        } else {
            // If it's locked then the notifier must be being dropped.
            Ok(Async::Ready(()))
        }
    }
}

impl Drop for DropNotify {
    fn drop(&mut self) {
        loop {
            if let Async::Ready(ref mut inner) = self.inner.poll_lock() {
                inner.dropped = true;
                if let Some(task) = inner.task_rx.take() {
                    task.notify();
                }
                return;
            }
            // The other thread (very temporarily) must have the lock. Spin until we get it.
            thread::yield_now();
        }
    }
}

