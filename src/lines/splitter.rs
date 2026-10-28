//! Bytes -> complete lines, plus the unterminated tail (for prompt detection).

pub const MAX_LINE: usize = 64 * 1024;

#[derive(Debug, Default)]
pub struct LineSplitter {
    partial: Vec<u8>,
    pending_cr: bool,
}

impl LineSplitter {
    pub fn new() -> LineSplitter {
        LineSplitter::default()
    }

    /// Returns the complete lines in `bytes`. `\r\n` ends a line; a lone `\r` (progress bars)
    /// throws away the partial line; lines longer than 64 KiB are split.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = vec![];
        for &b in bytes {
            if self.pending_cr {
                self.pending_cr = false;
                if b == b'\n' {
                    out.push(self.take());
                    continue;
                }
                self.partial.clear();
            }
            match b {
                b'\n' => out.push(self.take()),
                b'\r' => self.pending_cr = true,
                _ => {
                    self.partial.push(b);
                    if self.partial.len() >= MAX_LINE {
                        out.push(self.take());
                    }
                }
            }
        }
        out
    }

    /// The unterminated tail, lossy UTF-8 ("" when there is none).
    pub fn partial(&self) -> String {
        String::from_utf8_lossy(&self.partial).into_owned()
    }

    /// At EOF: the unterminated tail as a last line, if any.
    pub fn finish(&mut self) -> Option<String> {
        self.pending_cr = false;
        if self.partial.is_empty() {
            None
        } else {
            Some(self.take())
        }
    }

    fn take(&mut self) -> String {
        let s = String::from_utf8_lossy(&self.partial).into_owned();
        self.partial.clear();
        s
    }
}

#[cfg(test)]
#[path = "splitter_tests.rs"]
mod tests;
