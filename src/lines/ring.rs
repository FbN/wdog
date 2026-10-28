//! Fixed-size line buffers for `--tail`, `--head` and `last_lines`.

use std::collections::VecDeque;

#[derive(Debug, Clone, Default)]
pub struct Tail<T> {
    cap: usize,
    buf: VecDeque<T>,
}

impl<T: Clone> Tail<T> {
    pub fn new(cap: usize) -> Tail<T> {
        Tail {
            cap,
            buf: VecDeque::with_capacity(cap.min(1024)),
        }
    }
    pub fn push(&mut self, v: T) {
        if self.cap == 0 {
            return;
        }
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(v);
    }
    pub fn items(&self) -> Vec<T> {
        self.buf.iter().cloned().collect()
    }
    pub fn last(&self) -> Option<&T> {
        self.buf.back()
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Head<T> {
    cap: usize,
    buf: Vec<T>,
}

impl<T: Clone> Head<T> {
    pub fn new(cap: usize) -> Head<T> {
        Head { cap, buf: Vec::new() }
    }
    pub fn push(&mut self, v: T) {
        if self.buf.len() < self.cap {
            self.buf.push(v);
        }
    }
    pub fn items(&self) -> Vec<T> {
        self.buf.clone()
    }
    pub fn is_full(&self) -> bool {
        self.buf.len() >= self.cap
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

#[cfg(test)]
#[path = "ring_tests.rs"]
mod tests;
