//! Aggregate a job's members into one sample, and compare two samples (AN-1, AN-2).

use super::{ProcEntry, TtyRead};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub pid: i32,
    pub start_ticks: u64,
    pub cpu_ticks: u64,
    pub io: Option<u64>,
    pub state: char,
    pub cmdline: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub at: Instant,
    pub members: Vec<Member>,
    pub cpu_ticks: u64,
    /// Sum over readable members only.
    pub io_bytes: u64,
    /// Members with unreadable io.
    pub io_unknown: usize,
    /// Members blocked reading a terminal.
    pub tty_read: Vec<i32>,
}

impl Sample {
    pub fn empty(at: Instant) -> Sample {
        Sample {
            at,
            members: vec![],
            cpu_ticks: 0,
            io_bytes: 0,
            io_unknown: 0,
            tty_read: vec![],
        }
    }
    #[cfg(test)]
    pub fn pids(&self) -> Vec<i32> {
        self.members.iter().map(|m| m.pid).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Cpu(u64),
    Io(u64),
    /// A member appeared.
    Spawn,
    None,
    /// No change seen, and no member had readable I/O.
    Unknown,
}

pub fn aggregate(entries: &[ProcEntry], members: &[i32], at: Instant) -> Sample {
    let mut s = Sample::empty(at);
    for e in entries.iter().filter(|e| members.contains(&e.pid)) {
        s.cpu_ticks += e.cpu_ticks;
        let io = e.io.map(|c| c.rchar + c.wchar);
        match io {
            Some(b) => s.io_bytes += b,
            None => s.io_unknown += 1,
        }
        if e.tty_read == TtyRead::Yes {
            s.tty_read.push(e.pid);
        }
        s.members.push(Member {
            pid: e.pid,
            start_ticks: e.start_ticks,
            cpu_ticks: e.cpu_ticks,
            io,
            state: e.state,
            cmdline: e.cmdline.clone(),
        });
    }
    s
}

/// Compared per member: a process that exits takes its counters with it, so falling sums must
/// not hide progress elsewhere.
pub fn activity(prev: &Sample, cur: &Sample) -> Activity {
    let mut cpu = 0;
    let mut io = 0;
    let mut spawned = false;
    for m in &cur.members {
        match prev
            .members
            .iter()
            .find(|p| p.pid == m.pid && p.start_ticks == m.start_ticks)
        {
            Some(p) => {
                cpu += m.cpu_ticks.saturating_sub(p.cpu_ticks);
                if let (Some(a), Some(b)) = (p.io, m.io) {
                    io += b.saturating_sub(a);
                }
            }
            None => spawned = true,
        }
    }
    if cpu > 0 {
        Activity::Cpu(cpu)
    } else if io > 0 {
        Activity::Io(io)
    } else if spawned {
        Activity::Spawn
    } else if cur.members.is_empty() || cur.members.iter().all(|m| m.io.is_none()) {
        Activity::Unknown
    } else {
        Activity::None
    }
}

#[cfg(test)]
#[path = "sample_tests.rs"]
mod tests;
