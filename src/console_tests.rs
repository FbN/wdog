use super::*;
use std::io;

struct Broken(usize);
impl Write for Broken {
    fn write(&mut self, _b: &[u8]) -> io::Result<usize> {
        self.0 += 1;
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn broken_pipe_marks_dead_and_drops_later_writes() {
    // runs on its own test thread, so the thread-local flag is fresh
    let mut w = Broken(0);
    write_test(&mut w, "a");
    assert!(out_dead());
    let calls = w.0;
    write_test(&mut w, "b");
    assert_eq!(w.0, calls, "no write after the stream died");
}
