//! LocalChannel<T>: an unbounded single-threaded channel built on Rc + RefCell + Cell.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

struct Inner<T> {
    queue: RefCell<VecDeque<T>>,
    waker: Cell<Option<Waker>>,
    senders: Cell<usize>,
}

pub struct Sender<T> {
    inner: Rc<Inner<T>>,
}

pub struct Receiver<T> {
    inner: Rc<Inner<T>>,
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Rc::new(Inner {
        queue: RefCell::new(VecDeque::new()),
        waker: Cell::new(None),
        senders: Cell::new(1),
    });
    (Sender { inner: inner.clone() }, Receiver { inner })
}

impl<T> Sender<T> {
    pub fn send(&self, v: T) {
        self.inner.queue.borrow_mut().push_back(v);
        if let Some(w) = self.inner.waker.take() {
            w.wake();
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.inner.senders.set(self.inner.senders.get() + 1);
        Sender {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.inner.senders.set(self.inner.senders.get() - 1);
        if self.inner.senders.get() == 0 {
            if let Some(w) = self.inner.waker.take() {
                w.wake();
            }
        }
    }
}

impl<T> Receiver<T> {
    /// Next value, or None once every sender is dropped and the queue is empty.
    pub async fn recv(&self) -> Option<T> {
        poll_fn(|cx| {
            if let Some(v) = self.inner.queue.borrow_mut().pop_front() {
                return Poll::Ready(Some(v));
            }
            if self.inner.senders.get() == 0 {
                return Poll::Ready(None);
            }
            self.inner.waker.set(Some(cx.waker().clone()));
            Poll::Pending
        })
        .await
    }

    pub fn try_recv(&self) -> Option<T> {
        self.inner.queue.borrow_mut().pop_front()
    }
}

#[cfg(test)]
#[path = "channel_tests.rs"]
mod tests;
