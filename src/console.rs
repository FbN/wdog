//! The only writer to stdout and stderr. A write error (EPIPE: the reader went away) marks the
//! stream dead and later writes are dropped; wdog itself keeps running. Never panics.

use std::cell::Cell;
use std::io::Write;

thread_local! {
    static OUT_DEAD: Cell<bool> = const { Cell::new(false) };
    static ERR_DEAD: Cell<bool> = const { Cell::new(false) };
}

fn write_to(w: &mut dyn Write, dead: &'static std::thread::LocalKey<Cell<bool>>, s: &str) {
    if dead.with(|d| d.get()) {
        return;
    }
    let mut buf = Vec::with_capacity(s.len() + 1);
    buf.extend_from_slice(s.as_bytes());
    buf.push(b'\n');
    if w.write_all(&buf).and_then(|_| w.flush()).is_err() {
        dead.with(|d| d.set(true));
    }
}

/// One line (a `\n` is added) to stdout, flushed.
pub fn out(s: &str) {
    write_to(&mut std::io::stdout().lock(), &OUT_DEAD, s);
}

/// One line to stderr, flushed.
pub fn err(s: &str) {
    write_to(&mut std::io::stderr().lock(), &ERR_DEAD, s);
}

pub fn flush() {
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
}

#[cfg(test)]
pub fn out_dead() -> bool {
    OUT_DEAD.with(|d| d.get())
}

#[cfg(test)]
pub(crate) fn write_test(w: &mut dyn Write, s: &str) {
    write_to(w, &OUT_DEAD, s)
}

#[cfg(test)]
#[path = "console_tests.rs"]
mod tests;
