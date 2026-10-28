//! Read `/proc` into `ProcEntry` values. Every read error (process gone, no permission) is
//! skipped; a snapshot never fails as a whole.

use super::{ProcEntry, TtyRead, io::parse_io, stat::parse_stat, tty::classify};
use std::fs;
use std::path::PathBuf;

/// Every process, with `stat` fields only (enough for trees and kill checks).
pub fn snapshot_basic() -> Vec<ProcEntry> {
    let Ok(dir) = fs::read_dir("/proc") else {
        return vec![];
    };
    let mut out = vec![];
    for d in dir.flatten() {
        let name = d.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        if let Some(e) = basic(pid) {
            out.push(e);
        }
    }
    out
}

/// Every process with all fields. The sampler uses `snapshot_basic` + `enrich` on members only.
#[cfg(test)]
pub fn snapshot(want_tty: bool) -> Vec<ProcEntry> {
    let mut v = snapshot_basic();
    for e in &mut v {
        enrich(e, want_tty);
    }
    v
}

fn basic(pid: i32) -> Option<ProcEntry> {
    let st = parse_stat(&fs::read_to_string(format!("/proc/{pid}/stat")).ok()?).ok()?;
    Some(ProcEntry {
        pid,
        ppid: st.ppid,
        pgid: st.pgrp,
        state: st.state,
        cpu_ticks: st.utime + st.stime,
        start_ticks: st.starttime,
        io: None,
        tty_read: TtyRead::No,
        cmdline: format!("[{}]", st.comm),
    })
}

/// Add io counters, cmdline and (optionally) the terminal-read check.
pub fn enrich(e: &mut ProcEntry, want_tty: bool) {
    let base = PathBuf::from(format!("/proc/{}", e.pid));
    e.io = fs::read_to_string(base.join("io"))
        .ok()
        .and_then(|s| parse_io(&s).ok());
    if let Ok(b) = fs::read(base.join("cmdline")) {
        let s: String = String::from_utf8_lossy(&b)
            .replace('\0', " ")
            .trim()
            .chars()
            .take(200)
            .collect();
        if !s.is_empty() {
            e.cmdline = s;
        }
    }
    if want_tty {
        let wchan = fs::read_to_string(base.join("wchan")).ok();
        let fd0 = fs::read_link(base.join("fd/0")).ok();
        e.tty_read = classify(e.state, wchan.as_deref(), fd0.as_deref());
    }
}

/// Start time of a process in clock ticks (0 if unreadable).
pub fn start_ticks(pid: i32) -> u64 {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| parse_stat(&s).ok())
        .map(|f| f.starttime)
        .unwrap_or(0)
}

pub fn self_start_ticks() -> u64 {
    start_ticks(std::process::id() as i32)
}

/// True when `pid` exists, is not a zombie and started at `ticks`.
pub fn pid_alive(pid: i32, ticks: u64) -> bool {
    if pid <= 0 {
        return false;
    }
    match fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| parse_stat(&s).ok())
    {
        Some(f) => f.state != 'Z' && (ticks == 0 || f.starttime == ticks),
        None => false,
    }
}

pub fn clk_tck() -> f64 {
    // SAFETY: sysconf has no preconditions.
    let t = unsafe { nix::libc::sysconf(nix::libc::_SC_CLK_TCK) };
    if t > 0 { t as f64 } else { 100.0 }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
