//! `/proc` as plain data. Everything here is pure except `read.rs`.

pub mod io;
pub mod read;
pub mod sample;
pub mod stat;
pub mod tree;
pub mod tty;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoCounters {
    pub rchar: u64,
    pub wchar: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtyRead {
    Yes,
    No,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcEntry {
    pub pid: i32,
    pub ppid: i32,
    pub pgid: i32,
    /// R S D Z T ...
    pub state: char,
    /// utime + stime
    pub cpu_ticks: u64,
    /// Field 22, detects PID reuse.
    pub start_ticks: u64,
    /// None = unreadable (NF-5).
    pub io: Option<IoCounters>,
    pub tty_read: TtyRead,
    pub cmdline: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error: {}", self.0)
    }
}
