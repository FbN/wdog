//! Async line reader over a pipe (NF-3: no buffering beyond one line).

use crate::core::Input;
use crate::lines::{Stream, splitter::LineSplitter};
use crate::rt::channel::Sender;
use futures_lite::AsyncReadExt;
use std::cell::Cell;
use std::os::fd::OwnedFd;
use std::time::Instant;

/// Shared by a step's two readers and its drain task.
pub struct ReadState {
    /// Readers that have not reached EOF yet.
    pub open: Cell<usize>,
    pub last_read: Cell<Instant>,
}

pub async fn read_lines(
    step: usize,
    attempt: u32,
    fd: OwnedFd,
    stream: Stream,
    tx: Sender<Input>,
    st: std::rc::Rc<ReadState>,
) {
    let reader = async_io::Async::new(std::fs::File::from(fd));
    if let Ok(mut r) = reader {
        let mut sp = LineSplitter::new();
        let mut last_partial = String::new();
        let mut buf = vec![0u8; 8192];
        loop {
            match r.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let now = Instant::now();
                    st.last_read.set(now);
                    for text in sp.push(&buf[..n]) {
                        tx.send(Input::Line {
                            step,
                            attempt,
                            stream,
                            text,
                        });
                    }
                    let p = sp.partial();
                    if p != last_partial {
                        tx.send(Input::Partial {
                            step,
                            stream,
                            text: p.clone(),
                            since: now,
                        });
                        last_partial = p;
                    }
                }
            }
        }
        if let Some(text) = sp.finish() {
            tx.send(Input::Line {
                step,
                attempt,
                stream,
                text,
            });
            tx.send(Input::Partial {
                step,
                stream,
                text: String::new(),
                since: Instant::now(),
            });
        }
    }
    st.open.set(st.open.get().saturating_sub(1));
}
