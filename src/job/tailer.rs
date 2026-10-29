//! Incremental reader of a growing file (by byte offset). Handles truncation.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Tailer {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
}

impl Tailer {
    pub fn from_start(path: &Path) -> Tailer {
        Tailer {
            path: path.to_path_buf(),
            offset: 0,
            partial: vec![],
        }
    }

    pub fn from_end(path: &Path) -> Tailer {
        let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        Tailer {
            path: path.to_path_buf(),
            offset: len,
            partial: vec![],
        }
    }

    /// New complete lines since the last call.
    pub fn read_lines(&mut self) -> Vec<String> {
        let Ok(mut f) = File::open(&self.path) else {
            return vec![];
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        if len == self.offset || f.seek(SeekFrom::Start(self.offset)).is_err() {
            return vec![];
        }
        let mut buf = vec![];
        if f.take(len - self.offset).read_to_end(&mut buf).is_err() {
            return vec![];
        }
        self.offset += buf.len() as u64;
        self.partial.extend_from_slice(&buf);
        split_complete(&mut self.partial)
    }
}

/// Pure: take the complete lines out of `buf`, leave the unterminated tail.
pub fn split_complete(buf: &mut Vec<u8>) -> Vec<String> {
    let Some(last_nl) = buf.iter().rposition(|b| *b == b'\n') else {
        return vec![];
    };
    let rest = buf.split_off(last_nl + 1);
    let done = std::mem::replace(buf, rest);
    done[..done.len() - 1]
        .split(|b| *b == b'\n')
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect()
}

/// The last `n` lines of a file, reading backwards in 8 KiB blocks.
pub fn last_lines(path: &Path, n: usize) -> std::io::Result<Vec<String>> {
    let len = std::fs::metadata(path)?.len();
    Ok(lines_before(path, len, n)?.0)
}

/// The last `n` lines of the first `end` bytes of a file, and whether they
/// reach the start of the file.
pub fn lines_before(path: &Path, end: u64, n: usize) -> std::io::Result<(Vec<String>, bool)> {
    let mut f = File::open(path)?;
    let mut pos = end.min(f.metadata()?.len());
    let mut data: Vec<u8> = vec![];
    while pos > 0 {
        let step = pos.min(8192);
        pos -= step;
        f.seek(SeekFrom::Start(pos))?;
        let mut block = vec![0u8; step as usize];
        f.read_exact(&mut block)?;
        block.extend_from_slice(&data);
        data = block;
        let nl = data.iter().filter(|b| **b == b'\n').count();
        if nl > n {
            break;
        }
    }
    if data.is_empty() {
        return Ok((vec![], true));
    }
    let text = String::from_utf8_lossy(&data);
    let lines: Vec<&str> = text.strip_suffix('\n').unwrap_or(&text).split('\n').collect();
    let from = lines.len().saturating_sub(n);
    let lines = lines[from..].iter().map(|s| s.to_string()).collect();
    Ok((lines, pos == 0 && from == 0))
}

#[cfg(test)]
#[path = "tailer_tests.rs"]
mod tests;
